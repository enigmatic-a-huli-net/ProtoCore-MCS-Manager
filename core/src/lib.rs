//! # pcm-core
//!
//! Вся логика ProtoCore-MCS-Manager без привязки к интерфейсу.
//! Каждый модуль отвечает за одну область:
//!
//! - [`config`]  — настройки менеджера и список серверов (JSON рядом с .exe)
//! - [`detect`]  — автоопределение ядра и версии Minecraft по папке сервера
//! - [`launch`]  — сборка команды запуска (авто или свой .bat) и пресеты JVM-флагов
//! - [`process`] — запуск/остановка процесса сервера, консоль, статус
//! - [`mods`]    — моды: метаданные, иконки, зависимости, конфликты, клиентские моды
//! - [`plugins`] — плагины Bukkit/Paper: метаданные и зависимости
//! - [`backup`]  — снапшоты restic и откат
//! - [`analyze`] — разбор логов и краш-репортов, сжатый лог
//! - [`props`]   — чтение и запись server.properties
//! - [`update`]  — обновление самого менеджера с GitHub
//!
//! Интерфейс (src-tauri + ui) только вызывает функции отсюда.

pub mod advice;
pub mod analyze;
pub mod backup;
pub mod bisect;
pub mod catalog;
pub mod cfgfiles;
pub mod cfghints;
pub mod changes;
pub mod config;
pub mod cores;
pub mod detect;
pub mod error;
pub mod java;
pub mod launch;
pub mod logs;
pub mod modpack;
pub mod mods;
pub mod net;
pub mod notify;
pub mod network;
pub mod players;
pub mod plugins;
pub mod preflight;
pub mod secret;
pub mod sound;
pub mod process;
pub mod props;
pub mod schedule;
pub mod tps;
pub mod update;
pub mod util;

pub use error::{Error, Result};
pub use sysinfo;
