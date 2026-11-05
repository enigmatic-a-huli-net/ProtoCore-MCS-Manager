//! Советы по оптимизации: память, Java, флаги JVM, server.properties, полезные моды.
//! Каждый совет может нести действие, которое интерфейс выполняет одной кнопкой.

use serde::Serialize;

use crate::config::{JvmPreset, ServerEntry};
use crate::detect::CoreKind;
use crate::mods::{ClientConfidence, ModReport};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    InstallMod { slug: String, name: String },
    SetProps { changes: Vec<(String, String)> },
    SetMemory { mb: u32 },
    SetPreset { preset: String },
    Command { cmd: String },
    OpenTab { tab: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct Advice {
    pub id: String,
    /// warn — стоит исправить, tip — полезно, ok — уже хорошо
    pub level: String,
    pub title: String,
    pub detail: String,
    pub action: Option<Action>,
    pub action_label: String,
}

pub struct Input<'a> {
    pub server: &'a ServerEntry,
    pub mods: Option<&'a ModReport>,
    pub plugin_names: Vec<String>,
    pub props: Vec<(String, String)>,
    pub system_ram_mb: u64,
    pub java_major: Option<u32>,
    pub lag_warnings: usize,
}

fn adv(id: &str, level: &str, title: String, detail: String, action: Option<Action>, label: &str) -> Advice {
    Advice { id: id.into(), level: level.into(), title, detail, action, action_label: label.into() }
}

/// «openjdk version "21.0.4" 2024-07-16» → 21, «java version "1.8.0_401"» → 8.
pub fn java_major(version_line: &str) -> Option<u32> {
    let v = version_line.split('"').nth(1)?;
    let mut it = v.split(['.', '_', '-', '+']);
    let first: u32 = it.next()?.parse().ok()?;
    if first == 1 {
        it.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// Какая Java нужна для версии Minecraft.
pub fn java_needed(mc: &str) -> Option<u32> {
    let p: Vec<u32> = mc.split('.').filter_map(|x| x.parse().ok()).collect();
    match p.as_slice() {
        [1, minor, patch, ..] => Some(if (*minor, *patch) >= (20, 5) { 21 } else if *minor >= 18 { 17 } else if *minor == 17 { 16 } else { 8 }),
        [1, minor] => Some(if *minor >= 21 { 21 } else if *minor >= 18 { 17 } else { 8 }),
        [major, ..] if *major >= 25 => Some(21),
        _ => None,
    }
}

fn prop<'a>(props: &'a [(String, String)], k: &str) -> Option<&'a str> {
    props.iter().find(|(x, _)| x == k).map(|(_, v)| v.as_str())
}

pub fn advise(i: &Input) -> Vec<Advice> {
    let s = i.server;
    let mut out = Vec::new();
    let mod_count = i.mods.map(|r| r.enabled_count).unwrap_or(0);
    let hybrid = matches!(s.core, CoreKind::Youer | CoreKind::Mohist | CoreKind::Arclight);

    // ---------- память
    let rec_gb: u32 = match mod_count {
        0 if !s.core.supports_mods() => 4,
        0..=49 => 4,
        50..=119 => 6,
        120..=199 => 8,
        _ => 10,
    } + if hybrid && !i.plugin_names.is_empty() { 1 } else { 0 };
    let mem = s.launch.memory_mb;
    let sys = i.system_ram_mb.max(1) as u32;
    let max_ok = (sys as f32 * 0.75) as u32;
    if mem + 1024 < rec_gb * 1024 && rec_gb * 1024 <= max_ok {
        out.push(adv(
            "memory_low",
            "warn",
            format!("Мало памяти: {} ГБ на {} модов", mem / 1024, mod_count),
            format!("Для такой сборки обычно нужно около {rec_gb} ГБ. Нехватка памяти даёт лаги сборщика мусора и падения OutOfMemory."),
            Some(Action::SetMemory { mb: rec_gb * 1024 }),
            "Поставить рекомендуемое",
        ));
    } else if mem > max_ok && i.system_ram_mb > 0 {
        out.push(adv(
            "memory_high",
            "warn",
            format!("Серверу отдано {} ГБ из {} ГБ ОЗУ", mem / 1024, sys / 1024),
            "Системе и самой Java нужен запас сверх -Xmx. Если памяти не хватит Windows, начнётся подкачка — это хуже, чем немного меньше -Xmx.".into(),
            Some(Action::SetMemory { mb: (max_ok / 512) * 512 }),
            "Уменьшить до 75% ОЗУ",
        ));
    } else if mod_count > 0 {
        out.push(adv("memory_ok", "ok", format!("Памяти достаточно: {} ГБ", mem / 1024), String::new(), None, ""));
    }

    // ---------- JVM и Java
    if s.launch.mode == crate::config::LaunchMode::Auto && s.launch.jvm_preset == JvmPreset::None {
        out.push(adv(
            "jvm_preset",
            "tip",
            "Не выбран набор флагов JVM".into(),
            "Флаги Aikar (G1GC) заметно сглаживают паузы сборщика мусора на большинстве серверов.".into(),
            Some(Action::SetPreset { preset: "aikar".into() }),
            "Включить флаги Aikar",
        ));
    }
    if let (Some(have), Some(need)) = (i.java_major, java_needed(&s.mc_version)) {
        if have < need {
            out.push(adv(
                "java_old",
                "warn",
                format!("Java {have} слишком старая для Minecraft {}", s.mc_version),
                format!("Нужна Java {need} или новее. Укажи путь к ней на вкладке «Запуск»."),
                Some(Action::OpenTab { tab: "launch".into() }),
                "Открыть «Запуск»",
            ));
        } else if s.launch.jvm_preset == JvmPreset::Zgc && have < 21 {
            out.push(adv(
                "zgc_java",
                "warn",
                "ZGC выбран, но Java старше 21".into(),
                "Generational ZGC появился в Java 21. Переключись на флаги Aikar или обнови Java.".into(),
                Some(Action::SetPreset { preset: "aikar".into() }),
                "Переключить на Aikar",
            ));
        }
    }

    // ---------- server.properties
    let mut prop_changes = Vec::new();
    let vd = prop(&i.props, "view-distance").and_then(|v| v.parse::<u32>().ok());
    let sd = prop(&i.props, "simulation-distance").and_then(|v| v.parse::<u32>().ok());
    let nct = prop(&i.props, "network-compression-threshold").and_then(|v| v.parse::<i32>().ok());
    if vd.is_some_and(|v| v > 10) {
        prop_changes.push(("view-distance".to_string(), "10".to_string()));
    }
    if sd.is_some_and(|v| v > 6) && mod_count >= 50 {
        prop_changes.push(("simulation-distance".to_string(), "6".to_string()));
    }
    if nct.is_some_and(|v| !(128..=512).contains(&v)) {
        prop_changes.push(("network-compression-threshold".to_string(), "256".to_string()));
    }
    if !prop_changes.is_empty() {
        let list = prop_changes.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ");
        out.push(adv(
            "props",
            "warn",
            "Настройки сервера дают лишнюю нагрузку".into(),
            format!("Рекомендуется: {list}. Дальность прорисовки и симуляции — главный множитель нагрузки на процессор и канал."),
            Some(Action::SetProps { changes: prop_changes }),
            "Применить",
        ));
    } else if !i.props.is_empty() {
        out.push(adv("props_ok", "ok", "Дальность прорисовки и сжатие сети в норме".into(), String::new(), None, ""));
    }

    // ---------- моды
    if let Some(r) = i.mods {
        let has = |ids: &[&str]| {
            r.mods.iter().filter(|m| m.enabled).any(|m| m.meta.ids.iter().any(|x| ids.contains(&x.to_lowercase().as_str())))
                || i.plugin_names.iter().any(|p| ids.contains(&p.to_lowercase().as_str()))
        };
        let client = r.mods.iter().filter(|m| m.enabled && m.meta.client == ClientConfidence::Definite).count();
        if client > 0 {
            out.push(adv(
                "client_mods",
                "warn",
                format!("Включено клиентских модов: {client}"),
                "Они не нужны серверу и могут ронять запуск. Их можно убрать кнопкой на вкладке «Моды».".into(),
                Some(Action::OpenTab { tab: "mods".into() }),
                "Открыть «Моды»",
            ));
        }
        if !r.missing.is_empty() || !r.wrong_versions.is_empty() {
            out.push(adv(
                "deps",
                "warn",
                format!("Проблемы с зависимостями: нет {} · не та версия {}", r.missing.len(), r.wrong_versions.len()),
                "Сервер с такими модами, скорее всего, не запустится.".into(),
                Some(Action::OpenTab { tab: "mods".into() }),
                "Открыть «Моды»",
            ));
        }

        let loader = s.core.mod_loader();
        let mut suggest: Vec<(&str, &[&str], &str, &str)> = vec![
            ("modernfix", &["modernfix"], "ModernFix", "Ускоряет запуск и снижает потребление памяти — особенно на больших сборках."),
            ("ferrite-core", &["ferritecore"], "FerriteCore", "Сильно уменьшает потребление ОЗУ моделями блоков."),
            ("spark", &["spark"], "spark", "Профилировщик: показывает, какой мод или чанк грузит сервер (`spark profiler`)."),
            ("chunky", &["chunky"], "Chunky", "Предгенерация мира: игроки не ждут генерации, меньше лагов при исследовании."),
        ];
        match loader {
            Some(crate::detect::ModLoader::Fabric) | Some(crate::detect::ModLoader::Quilt) => {
                suggest.push(("lithium", &["lithium"], "Lithium", "Оптимизация игровой логики сервера без изменения поведения."));
                suggest.push(("c2me-fabric", &["c2me"], "C2ME", "Многопоточная генерация и загрузка чанков."));
                suggest.push(("krypton", &["krypton"], "Krypton", "Оптимизация сетевого стека."));
            }
            Some(_) => {
                suggest.push(("clumps", &["clumps"], "Clumps", "Склеивает сферы опыта — меньше сущностей и лагов у ферм."));
                if !hybrid {
                    suggest.push(("lithium", &["lithium", "canary", "radium"], "Lithium", "Оптимизация игровой логики сервера (есть версия для NeoForge)."));
                }
            }
            None => {}
        }
        if s.core.supports_mods() {
            for (slug, ids, name, why) in suggest {
                if !has(ids) {
                    out.push(adv(
                        &format!("mod_{slug}"),
                        "tip",
                        format!("Поставь {name}"),
                        why.into(),
                        Some(Action::InstallMod { slug: slug.into(), name: name.into() }),
                        "Установить",
                    ));
                }
            }
            if has(&["chunky"]) {
                out.push(adv(
                    "pregen",
                    "tip",
                    "Предгенерируй мир через Chunky".into(),
                    "Сгенерируй заранее круг радиусом 3000 блоков вокруг спавна — во время генерации сервер будет нагружен, лучше делать ночью.".into(),
                    Some(Action::Command { cmd: "chunky radius 3000".into() }),
                    "Задать радиус 3000",
                ));
            }
        }
    }

    // ---------- лаги
    if i.lag_warnings >= 10 {
        out.push(adv(
            "lag",
            "warn",
            format!("Сервер не успевал {} раз (Can't keep up)", i.lag_warnings),
            "Запусти профилировщик spark на 2 минуты во время лагов — он покажет, что именно нагружает сервер.".into(),
            Some(Action::Command { cmd: "spark profiler start --timeout 120".into() }),
            "Запустить spark на 2 мин",
        ));
    }

    // ---------- надёжность
    if !s.backup.snapshot_on_start {
        out.push(adv(
            "snapshots",
            "tip",
            "Снапшоты при запуске выключены".into(),
            "Снапшот перед запуском почти не занимает места и позволяет откатиться одной кнопкой.".into(),
            Some(Action::OpenTab { tab: "backups".into() }),
            "Открыть «Снапшоты»",
        ));
    }
    if mod_count >= 100 && !s.schedule.restart_enabled {
        out.push(adv(
            "restarts",
            "tip",
            "Плановые рестарты".into(),
            "Большие сборки со временем копят утечки памяти. Рестарт раз в 8–12 часов с предупреждением игрокам держит сервер свежим.".into(),
            Some(Action::OpenTab { tab: "schedule".into() }),
            "Открыть «Расписание»",
        ));
    }

    // порядок: проблемы, советы, «в порядке»
    let rank = |l: &str| match l {
        "warn" => 0,
        "tip" => 1,
        _ => 2,
    };
    out.sort_by_key(|a| rank(&a.level));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_versions() {
        assert_eq!(java_major(r#"openjdk version "21.0.4" 2024-07-16"#), Some(21));
        assert_eq!(java_major(r#"java version "1.8.0_401""#), Some(8));
        assert_eq!(java_needed("1.21.1"), Some(21));
        assert_eq!(java_needed("1.20.1"), Some(17));
        assert_eq!(java_needed("1.20.6"), Some(21));
        assert_eq!(java_needed("1.12.2"), Some(8));
        assert_eq!(java_needed("26.1"), Some(21));
    }

    #[test]
    fn advises() {
        let mut s = ServerEntry { core: CoreKind::NeoForge, mc_version: "1.21.1".into(), ..Default::default() };
        s.launch.memory_mb = 4096;
        let mut v: Vec<crate::mods::ModInfo> = Vec::new();
        for n in 0..210 {
            let mut meta = crate::mods::read_meta(std::path::Path::new("/none"));
            meta.ids = vec![format!("m{n}")];
            v.push(crate::mods::ModInfo {
                file: format!("m{n}.jar"),
                enabled: true,
                size: 0,
                id: format!("m{n}"),
                meta,
                disabled_reason: None,
                disabled_detail: None,
                required_by: vec![],
                missing_deps: vec![],
                wrong_deps: vec![],
                problems: vec![],
            });
        }
        let r = crate::mods::analyze(&mut v, None);
        let a = advise(&Input {
            server: &s,
            mods: Some(&r),
            plugin_names: vec![],
            props: vec![("view-distance".into(), "16".into()), ("simulation-distance".into(), "10".into())],
            system_ram_mb: 32768,
            java_major: Some(17),
            lag_warnings: 25,
        });
        let ids: Vec<&str> = a.iter().map(|x| x.id.as_str()).collect();
        for want in ["memory_low", "java_old", "props", "mod_modernfix", "mod_spark", "lag", "restarts"] {
            assert!(ids.contains(&want), "нет {want}: {ids:?}");
        }
        let m = a.iter().find(|x| x.id == "memory_low").unwrap();
        assert_eq!(m.action, Some(Action::SetMemory { mb: 10240 }));
        assert_eq!(a[0].level, "warn");
    }
}
