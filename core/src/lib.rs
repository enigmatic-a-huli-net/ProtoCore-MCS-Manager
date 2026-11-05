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
//! - [`props`]   — чтение и запись server.properties
//!
//! Интерфейс (src-tauri + ui) только вызывает функции отсюда.

pub mod backup;
pub mod config;
pub mod detect;
pub mod error;
pub mod launch;
pub mod mods;
pub mod plugins;
pub mod process;
pub mod props;
pub mod util;

pub use error::{Error, Result};
