//! Настройки менеджера и список серверов.
//!
//! Менеджер «портативный»: всё хранится рядом с .exe.
//!
//! ```text
//! ProtoCore-MCS-Manager.exe
//! data/config.json        ← этот модуль
//! restic/<сервер>/        ← репозитории снапшотов (по умолчанию)
//! tools/restic.exe        ← скачивается автоматически
//! branding/logo.png       ← свой логотип (необязательно)
//! branding/splash.gif     ← своя анимация загрузки (необязательно)
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::detect::CoreKind;
use crate::Result;

/// Все пути менеджера, вычисляются от папки с .exe.
#[derive(Debug, Clone, Serialize)]
pub struct AppPaths {
    pub base: PathBuf,
    pub data: PathBuf,
    pub restic_root: PathBuf,
    pub tools: PathBuf,
    pub branding: PathBuf,
}

impl AppPaths {
    pub fn from_base(base: impl Into<PathBuf>) -> Self {
        let base = base.into();
        Self {
            data: base.join("data"),
            restic_root: base.join("restic"),
            tools: base.join("tools"),
            branding: base.join("branding"),
            base,
        }
    }

    /// Папка, где лежит исполняемый файл.
    pub fn from_exe() -> Self {
        let base = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        Self::from_base(base)
    }

    pub fn config_file(&self) -> PathBuf {
        self.data.join("config.json")
    }

    pub fn ensure(&self) -> Result<()> {
        for d in [&self.data, &self.tools, &self.branding] {
            fs::create_dir_all(d)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- тема

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
    System,
}

/// Настройки внешнего вида. Цвета — CSS-строки (`#7c5cff`).
/// Пустая строка значит «взять из встроенной темы».
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub mode: ThemeMode,
    pub accent: String,
    pub background: String,
    pub surface: String,
    pub text: String,
    /// Скругление углов, px
    pub radius: u32,
    /// Базовый размер шрифта, px
    pub font_size: u32,
    /// Включить анимации интерфейса
    pub animations: bool,
    /// Скорость анимаций: 0.5 — медленно, 1 — обычно, 2 — быстро
    pub anim_speed: f32,
    /// Показывать анимации, даже если в Windows включено «уменьшить движение»
    pub anim_force: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            accent: "#7c5cff".into(),
            background: String::new(),
            surface: String::new(),
            text: String::new(),
            radius: 10,
            font_size: 14,
            animations: true,
            anim_speed: 1.0,
            anim_force: false,
        }
    }
}

// ---------------------------------------------------------------- запуск

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    /// Менеджер сам собирает команду из настроек ниже
    Auto,
    /// Запускается указанный пользователем .bat / .cmd / .sh
    Script,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JvmPreset {
    /// Флаги Aikar (G1GC) — проверенный вариант для большинства серверов
    Aikar,
    /// Generational ZGC (Java 21+) — минимальные паузы, нужно больше ОЗУ
    Zgc,
    /// Без пресета, только свои флаги
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchSettings {
    pub mode: LaunchMode,
    /// Путь к своему скрипту (для `LaunchMode::Script`)
    pub script: String,
    /// Путь к java(.exe). Пусто = `java` из PATH
    pub java_path: String,
    /// ОЗУ, МБ. Xms = Xmx по рекомендации
    pub memory_mb: u32,
    pub jvm_preset: JvmPreset,
    /// Свои дополнительные флаги JVM, через пробел
    pub extra_jvm_args: String,
    /// Аргументы самого сервера (обычно `nogui`)
    pub server_args: String,
    /// Принудительно указанный jar (если автоопределение ошиблось). Пусто = авто
    pub jar_override: String,
    /// Сколько секунд ждать мягкой остановки перед принудительной
    pub stop_timeout_sec: u32,
}

impl Default for LaunchSettings {
    fn default() -> Self {
        Self {
            mode: LaunchMode::Auto,
            script: String::new(),
            java_path: String::new(),
            memory_mb: 8192,
            jvm_preset: JvmPreset::Aikar,
            extra_jvm_args: String::new(),
            server_args: "nogui".into(),
            jar_override: String::new(),
            stop_timeout_sec: 60,
        }
    }
}

// ---------------------------------------------------------------- бэкапы

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupSettings {
    /// Делать снапшот перед каждым запуском
    pub snapshot_on_start: bool,
    /// Своя папка репозитория. Пусто = `<папка менеджера>/restic/<сервер>`
    pub repo_path: String,
    /// Пароль репозитория (генерируется автоматически)
    pub password: String,
    /// Сколько последних снапшотов хранить (0 = все)
    pub keep_last: u32,
    /// Что не включать в снапшот (шаблоны restic, относительно папки сервера)
    pub excludes: Vec<String>,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            snapshot_on_start: true,
            repo_path: String::new(),
            password: String::new(),
            keep_last: 30,
            excludes: vec![
                "logs".into(),
                "crash-reports".into(),
                "session.lock".into(),
                "*.tmp".into(),
                ".cache".into(),
            ],
        }
    }
}

// ---------------------------------------------------------------- моды

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModSettings {
    /// Перед запуском переносить явно клиентские моды в mods_disabled/
    pub auto_disable_client: bool,
    /// Также переносить моды из чёрного списка («вероятно клиентские»)
    pub disable_blacklisted: bool,
    /// Свои id модов, которые считать клиентскими
    pub extra_client_ids: Vec<String>,
    /// id модов, которые никогда не трогать автоматически
    pub never_disable: Vec<String>,
    /// Автокарантин: если сервер упал из-за мода — перенести мод в mods_disabled/
    pub auto_quarantine: bool,
    /// Разрешить карантин не только клиентских модов.
    /// Осторожно: блоки и предметы убранного мода пропадут из мира (снапшот есть).
    pub quarantine_content: bool,
    /// После карантина запустить сервер снова
    pub quarantine_restart: bool,
    /// Сколько раз подряд можно отправлять моды в карантин, пока сервер не запустится
    pub quarantine_max_rounds: u32,
}

impl Default for ModSettings {
    fn default() -> Self {
        Self {
            auto_disable_client: true,
            disable_blacklisted: false,
            extra_client_ids: vec![],
            never_disable: vec![],
            auto_quarantine: true,
            quarantine_content: false,
            quarantine_restart: true,
            quarantine_max_rounds: 5,
        }
    }
}

// ---------------------------------------------------------------- расписание

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScheduleSettings {
    /// Опрашивать TPS/MSPT
    pub tps_enabled: bool,
    pub tps_interval_sec: u32,
    /// Плановый рестарт
    pub restart_enabled: bool,
    /// "interval" — каждые N часов работы; "times" — в указанное время
    pub restart_mode: String,
    pub restart_hours: u32,
    /// «06:00», «18:00»
    pub restart_times: Vec<String>,
    /// За сколько минут предупреждать игроков
    pub warn_minutes: Vec<u32>,
    /// Текст предупреждения, {min} — минуты
    pub warn_message: String,
    /// Не перезапускать, если на сервере есть игроки (переносится на позже)
    pub skip_if_players: bool,
    /// Ночной бэкап
    pub backup_enabled: bool,
    pub backup_time: String,
}

impl Default for ScheduleSettings {
    fn default() -> Self {
        Self {
            tps_enabled: true,
            tps_interval_sec: 30,
            restart_enabled: false,
            restart_mode: "interval".into(),
            restart_hours: 8,
            restart_times: vec!["06:00".into()],
            warn_minutes: vec![15, 5, 1],
            warn_message: "Сервер перезапустится через {min} мин. Плановый рестарт.".into(),
            skip_if_players: false,
            backup_enabled: false,
            backup_time: "04:00".into(),
        }
    }
}

// ---------------------------------------------------------------- сервер

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    /// Ядро: определяется автоматически, но можно переопределить вручную
    pub core: CoreKind,
    pub core_override: bool,
    pub mc_version: String,
    pub loader_version: String,
    pub launch: LaunchSettings,
    pub backup: BackupSettings,
    pub mods: ModSettings,
    /// Перезапускать при падении
    pub auto_restart: bool,
    pub schedule: ScheduleSettings,
}

impl Default for ServerEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            path: String::new(),
            core: CoreKind::Unknown,
            core_override: false,
            mc_version: String::new(),
            loader_version: String::new(),
            launch: LaunchSettings::default(),
            backup: BackupSettings::default(),
            mods: ModSettings::default(),
            auto_restart: false,
            schedule: ScheduleSettings::default(),
        }
    }
}

impl ServerEntry {
    pub fn dir(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    /// Папка репозитория restic для этого сервера.
    /// У каждого сервера всегда своя папка.
    pub fn restic_repo(&self, paths: &AppPaths) -> PathBuf {
        if self.backup.repo_path.trim().is_empty() {
            paths.restic_root.join(format!(
                "{}-{}",
                crate::util::safe_dir_name(&self.name),
                &self.id
            ))
        } else {
            // своя папка + подпапка сервера, чтобы два сервера никогда не делили репозиторий
            PathBuf::from(self.backup.repo_path.trim()).join(&self.id)
        }
    }
}

// ---------------------------------------------------------------- конфиг целиком

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// "ru" или "en"
    pub language: String,
    pub theme: Theme,
    /// Закрытие окна сворачивает в трей (окно выгружается, сервер работает)
    pub close_to_tray: bool,
    /// Сколько строк консоли держать в памяти на сервер
    pub console_lines: usize,
    /// Проверять обновления менеджера при запуске
    pub check_updates: bool,
    /// Репозиторий GitHub, откуда берутся обновления
    pub update_repo: String,
    /// API-ключ CurseForge (необязательно; без него CurseForge в каталоге недоступен)
    pub curseforge_key: String,
    /// Папка по умолчанию для новых серверов
    pub servers_root: String,
    /// Последний выбранный сервер
    pub selected: String,
    pub servers: Vec<ServerEntry>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: "ru".into(),
            theme: Theme::default(),
            close_to_tray: true,
            console_lines: 5000,
            check_updates: true,
            update_repo: crate::update::DEFAULT_REPO.into(),
            curseforge_key: String::new(),
            servers_root: String::new(),
            selected: String::new(),
            servers: vec![],
        }
    }
}

impl Config {
    pub fn load(paths: &AppPaths) -> Self {
        let file = paths.config_file();
        match fs::read_to_string(&file) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                // испорченный конфиг не теряем: сохраняем копию рядом
                let _ = fs::copy(&file, file.with_extension("json.broken"));
                log::error!("config.json повреждён ({e}), создан новый");
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }

    /// Сохранение через временный файл, чтобы конфиг не побился при сбое питания.
    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        paths.ensure()?;
        let file = paths.config_file();
        let tmp = file.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        fs::rename(&tmp, &file)?;
        Ok(())
    }

    pub fn server(&self, id: &str) -> Option<&ServerEntry> {
        self.servers.iter().find(|s| s.id == id)
    }

    pub fn server_mut(&mut self, id: &str) -> Option<&mut ServerEntry> {
        self.servers.iter_mut().find(|s| s.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_partial_json() {
        let t = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_base(t.path());
        let mut c = Config::default();
        c.servers.push(ServerEntry {
            id: "abc".into(),
            name: "Test".into(),
            ..Default::default()
        });
        c.save(&paths).unwrap();
        let c2 = Config::load(&paths);
        assert_eq!(c2.servers[0].name, "Test");

        // старый/неполный конфиг тоже читается — недостающие поля берутся по умолчанию
        fs::write(paths.config_file(), r#"{"language":"en","servers":[{"id":"x"}]}"#).unwrap();
        let c3 = Config::load(&paths);
        assert_eq!(c3.language, "en");
        assert_eq!(c3.servers[0].launch.memory_mb, 8192);
    }

    #[test]
    fn repo_per_server() {
        let paths = AppPaths::from_base("/m");
        let mut s = ServerEntry { id: "id1".into(), name: "A B".into(), ..Default::default() };
        assert_eq!(s.restic_repo(&paths), PathBuf::from("/m/restic/A_B-id1"));
        s.backup.repo_path = "/backups".into();
        assert_eq!(s.restic_repo(&paths), PathBuf::from("/backups/id1"));
    }
}
