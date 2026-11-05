//! Анализ логов и краш-репортов.
//!
//! - [`analyze`] ищет типичные ошибки и по возможности называет виновный мод;
//! - [`compress`] делает «сжатый лог»: только то, что нужно для разбора
//!   (версии, ошибки, первые строки стектрейсов, Suspected Mods), без тысяч строк загрузки.
//!
//! Виновный мод определяется так (от надёжного к менее надёжному):
//! 1. мод прямо назван в сообщении (Mixin apply for mod X, Mod X requires …);
//! 2. строка «Suspected Mods:» в краш-репорте NeoForge/Forge;
//! 3. Java-пакет из стектрейса совпадает с пакетом мода (com.simibubi.create → Create).

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;

use crate::mods::{ModInfo, ModReport};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

/// Насколько уверенно назван виновник.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Только «встречается в стектрейсе»
    Low,
    /// Назван в Suspected Mods
    Medium,
    /// Назван прямо в сообщении об ошибке
    High,
}

#[derive(Debug, Clone, Serialize)]
pub struct Culprit {
    pub file: String,
    pub name: String,
    pub id: String,
    pub confidence: Confidence,
    /// Мод клиентский — убирать его с сервера безопасно (у него нет блоков в мире)
    pub client_only: bool,
    /// Мод сейчас включён (false — уже в mods_disabled)
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// Машинный код проблемы: client_class, missing_dep, mixin, oom, port, java_version …
    pub code: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub hint: String,
    /// Строка лога, где это нашлось
    pub line: String,
    pub culprits: Vec<Culprit>,
    /// Сколько раз встретилось
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    pub source: String,
    pub source_path: String,
    pub findings: Vec<Finding>,
    pub compressed: String,
    pub total_lines: usize,
    pub errors: usize,
    pub warnings: usize,
    /// Сколько раз сервер жаловался «Can't keep up»
    pub lag_warnings: usize,
}

// ===================================================================== индекс модов

/// Быстрый поиск мода по id, имени, файлу и Java-пакету.
pub struct ModIndex<'a> {
    by_id: HashMap<String, &'a ModInfo>,
    packages: Vec<(String, &'a ModInfo)>,
}

impl<'a> ModIndex<'a> {
    pub fn new(report: Option<&'a ModReport>) -> Self {
        let mut by_id = HashMap::new();
        let mut packages = Vec::new();
        if let Some(r) = report {
            for m in &r.mods {
                for id in &m.meta.ids {
                    by_id.insert(id.to_lowercase(), m);
                }
                by_id.entry(m.meta.name.to_lowercase()).or_insert(m);
                by_id.entry(m.file.to_lowercase()).or_insert(m);
                for p in &m.meta.packages {
                    packages.push((format!("{p}."), m));
                }
            }
        }
        // длинные пакеты первыми: com.foo.bar точнее, чем com.foo
        packages.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        Self { by_id, packages }
    }

    pub fn find(&self, key: &str) -> Option<&'a ModInfo> {
        let k = key.trim().trim_matches(|c| c == '\'' || c == '"' || c == '(' || c == ')').to_lowercase();
        self.by_id.get(&k).copied()
    }

    pub fn by_class(&self, class: &str) -> Option<&'a ModInfo> {
        // не считаем «виновными» сам Minecraft, загрузчик и Java
        const SKIP: &[&str] = &[
            "net.minecraft.", "com.mojang.", "java.", "jdk.", "sun.", "net.neoforged.", "net.minecraftforge.",
            "net.fabricmc.", "org.spongepowered.", "cpw.mods.", "com.google.", "org.apache.", "io.netty.",
            "org.quiltmc.", "it.unimi.", "org.bukkit.", "org.spigotmc.", "io.papermc.", "com.llamalad7.",
        ];
        if SKIP.iter().any(|s| class.starts_with(s)) {
            return None;
        }
        self.packages.iter().find(|(p, _)| class.starts_with(p.as_str())).map(|(_, m)| *m)
    }
}

fn culprit(m: &ModInfo, confidence: Confidence) -> Culprit {
    Culprit {
        file: m.file.clone(),
        name: if m.meta.name.is_empty() { m.id.clone() } else { m.meta.name.clone() },
        id: m.id.clone(),
        confidence,
        client_only: m.meta.client != crate::mods::ClientConfidence::No,
        enabled: m.enabled,
    }
}

// ===================================================================== правила

fn re(s: &'static str) -> Regex {
    Regex::new(s).unwrap()
}

macro_rules! rx {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| re($pat))
        }
    };
}

rx!(r_frame, r"^\s*at\s+(?:[\w.$/@-]+//)?([\w$]+(?:\.[\w$]+)+)\.[\w$<>]+\(");
rx!(r_client_class, r"(?i)(?:Attempted to load class|NoClassDefFoundError:?|ClassNotFoundException:?)\s+([\w/$.]*net[/.]minecraft[/.]client[/.][\w/$.]+)");
rx!(r_invalid_dist, r"(?i)for invalid (?:dist|side) DEDICATED_SERVER");
rx!(r_mixin_mod, r"(?i)Mixin apply (?:for mod|failed) ([\w\-.]+?)(?:\.mixins?\.json|\s|:|$)");
rx!(r_mixin_cfg, r"(?i)(?:from mod|mixin config|config) ([\w\-]+)\.mixins?\.json");
rx!(r_fabric_requires, r"Mod '([^']+)' \(([\w\-]+)\) [\w.+\-]+ requires (?:version [^ ]+ of |any version of )?(?:mod )?'?([^'(,]+)'?\s*(?:\(([\w\-]+)\))?");
rx!(r_neo_missing, r"(?i)Mod ID: '([\w\-]+)', Requested by: '([\w\-]+)'");
rx!(r_forge_requires, r"(?i)Mod (\w+) requires (\w+) ([^\s]+)");
rx!(r_incompatible, r"(?i)(?:Mod|Incompatible mod set!?)\s*'?([\w\-]+)'? (?:is incompatible with|breaks) '?([\w\-]+)'?");
rx!(r_duplicate, r"(?i)(?:Found duplicate mods|duplicate mods? found|Found \d+ duplicate mods?|Duplicate mods found)");
rx!(r_suspected, r"(?i)^\s*Suspected Mods?:\s*(.+)$");
rx!(r_suspected_item, r"([^,()]+?)\s*\(([\w\-]+)\)");
rx!(r_java_ver, r"class file version (\d+)\.\d+\), this version of the Java Runtime only recognizes class file versions up to (\d+)");
rx!(r_cant_keep_up, r"Can't keep up! Is the server overloaded\? Running (\d+)ms or (\d+) ticks behind");
rx!(r_port, r"(?i)FAILED TO BIND TO PORT|Address already in use");
rx!(r_oom, r"OutOfMemoryError");
rx!(r_watchdog, r"(?i)(A single server tick took [\d.]+ seconds|Server Watchdog|ServerHangWatchdog)");
rx!(r_chunk, r"(?i)(Failed to (?:read|load) chunk|Couldn't load chunk|Chunk file at \[.+\] is in the wrong location|Region file .* corrupt)");
rx!(r_config, r"(?i)(Failed loading config file ([\w\-./\\]+)|Configuration file ([\w\-./\\]+) is not correct|ParsingException.*config)");
rx!(r_invalid_jar, r"(?i)(?:File|Mod file) ([\w\-.+ ]+\.jar) (?:is not a valid mod file|is corrupt|has an invalid)|zip END header not found|java\.util\.zip\.ZipException");
rx!(r_eula, r"You need to agree to the EULA");
rx!(r_tick_loop, r"(?i)Encountered an unexpected exception|Exception in server tick loop");
rx!(r_description, r"^Description:\s*(.+)$");
rx!(r_exception_head, r"^(?:Caused by: )?((?:[a-z][\w$]*\.)+[A-Z][\w$]*(?:Exception|Error|Throwable))(?::\s*(.*))?$");

// ===================================================================== анализ

struct Builder<'a> {
    idx: &'a ModIndex<'a>,
    map: BTreeMap<String, Finding>,
}

impl<'a> Builder<'a> {
    fn add(&mut self, key: String, f: impl FnOnce() -> Finding) -> &mut Finding {
        self.map
            .entry(key)
            .and_modify(|x| x.count += 1)
            .or_insert_with(f)
    }

    fn add_culprit(&mut self, key: &str, c: Culprit) {
        if let Some(f) = self.map.get_mut(key) {
            if let Some(x) = f.culprits.iter_mut().find(|x| x.file == c.file) {
                x.confidence = x.confidence.max(c.confidence);
            } else {
                f.culprits.push(c);
            }
        }
    }
}

fn finding(code: &str, sev: Severity, title: String, detail: String, hint: &str, line: &str) -> Finding {
    Finding {
        code: code.into(),
        severity: sev,
        title,
        detail,
        hint: hint.into(),
        line: line.chars().take(400).collect(),
        culprits: vec![],
        count: 1,
    }
}

/// Анализ текста лога или краш-репорта.
pub fn analyze_text(text: &str, idx: &ModIndex) -> Vec<Finding> {
    let lines: Vec<&str> = text.lines().collect();
    let mut b = Builder { idx, map: BTreeMap::new() };

    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim_end();

        // --- клиентский код на сервере
        if let Some(c) = r_client_class().captures(line) {
            let class = c[1].replace('/', ".");
            let key = "client_class".to_string();
            b.add(key.clone(), || {
                finding(
                    "client_class",
                    Severity::Error,
                    "Мод пытается загрузить клиентский код на сервере".into(),
                    format!("Класс {class} существует только в игре, не на сервере."),
                    "Такой мод нужно убрать с сервера — он нужен только игрокам. Это безопасно: у клиентских модов нет блоков в мире.",
                    line,
                )
            });
            // виновник — первый «не майнкрафтовский» кадр ниже
            if let Some(m) = first_mod_frame(&lines, i + 1, 40, b.idx) {
                b.add_culprit(&key, culprit(m, Confidence::High));
            }
            // или mixin-конфиг в той же строке
            if let Some(cc) = r_mixin_cfg().captures(line) {
                if let Some(m) = b.idx.find(&cc[1]) {
                    b.add_culprit(&key, culprit(m, Confidence::High));
                }
            }
            continue;
        }
        if r_invalid_dist().is_match(line) {
            let key = "client_class".to_string();
            b.add(key.clone(), || {
                finding(
                    "client_class",
                    Severity::Error,
                    "Мод пытается загрузить клиентский код на сервере".into(),
                    "Загрузчик отказался загружать клиентский класс на выделенном сервере.".into(),
                    "Убери этот мод с сервера — он нужен только игрокам.",
                    line,
                )
            });
            if let Some(cc) = r_mixin_cfg().captures(line) {
                if let Some(m) = b.idx.find(&cc[1]) {
                    b.add_culprit(&key, culprit(m, Confidence::High));
                }
            }
            if let Some(m) = first_mod_frame(&lines, i + 1, 40, b.idx) {
                b.add_culprit(&key, culprit(m, Confidence::Medium));
            }
            continue;
        }

        // --- mixin
        if let Some(c) = r_mixin_mod().captures(line) {
            let id = c[1].trim_end_matches(".mixins").to_string();
            let key = format!("mixin:{id}");
            b.add(key.clone(), || {
                finding(
                    "mixin",
                    Severity::Error,
                    format!("Мод «{id}» не смог встроиться в игру (Mixin)"),
                    "Обычно это несовместимая версия мода, конфликт с другим модом или клиентский мод.".into(),
                    "Проверь, что версия мода подходит под твою версию Minecraft и загрузчика; попробуй отключить мод.",
                    line,
                )
            });
            if let Some(m) = b.idx.find(&id) {
                b.add_culprit(&key, culprit(m, Confidence::High));
            }
            continue;
        }

        // --- отсутствующие зависимости
        if let Some(c) = r_neo_missing().captures(line) {
            let (dep, by) = (c[1].to_string(), c[2].to_string());
            let key = format!("missing:{dep}");
            b.add(key.clone(), || {
                finding(
                    "missing_dep",
                    Severity::Error,
                    format!("Не хватает мода «{dep}»"),
                    format!("Его требует «{by}»."),
                    "Установи нужный мод (вкладка «Моды» подскажет, если он просто выключен) или убери тот, кому он нужен.",
                    line,
                )
            });
            if let Some(m) = b.idx.find(&by) {
                b.add_culprit(&key, culprit(m, Confidence::Medium));
            }
            continue;
        }
        if let Some(c) = r_fabric_requires().captures(line) {
            let by = c[2].to_string();
            let dep = c.get(4).map(|x| x.as_str().to_string()).unwrap_or_else(|| c[3].trim().to_string());
            let key = format!("missing:{dep}");
            b.add(key.clone(), || {
                finding(
                    "missing_dep",
                    Severity::Error,
                    format!("Не хватает мода «{dep}» (или он не той версии)"),
                    format!("Его требует «{}».", &c[1]),
                    "Установи нужную версию зависимости или убери мод, которому она нужна.",
                    line,
                )
            });
            if let Some(m) = b.idx.find(&by) {
                b.add_culprit(&key, culprit(m, Confidence::Medium));
            }
            continue;
        }
        if let Some(c) = r_forge_requires().captures(line) {
            let (by, dep) = (c[1].to_string(), c[2].to_string());
            let key = format!("missing:{dep}");
            b.add(key.clone(), || {
                finding(
                    "missing_dep",
                    Severity::Error,
                    format!("Не хватает мода «{dep}» {}", &c[3]),
                    format!("Его требует «{by}»."),
                    "Установи нужную версию зависимости.",
                    line,
                )
            });
            if let Some(m) = b.idx.find(&by) {
                b.add_culprit(&key, culprit(m, Confidence::Medium));
            }
            continue;
        }

        if let Some(c) = r_incompatible().captures(line) {
            let (a, x) = (c[1].to_string(), c[2].to_string());
            let key = format!("incompatible:{a}:{x}");
            b.add(key.clone(), || {
                finding(
                    "incompatible",
                    Severity::Error,
                    format!("«{a}» несовместим с «{x}»"),
                    "Загрузчик отказался запускать эти моды вместе.".into(),
                    "Оставь только один из них.",
                    line,
                )
            });
            for id in [&a, &x] {
                if let Some(m) = b.idx.find(id) {
                    b.add_culprit(&key, culprit(m, Confidence::High));
                }
            }
            continue;
        }

        if r_duplicate().is_match(line) {
            b.add("duplicate".into(), || {
                finding(
                    "duplicate",
                    Severity::Error,
                    "Один и тот же мод установлен дважды".into(),
                    "Обычно это две версии одного jar в папке mods.".into(),
                    "Удали старую версию — вкладка «Моды» показывает дубликаты как конфликт.",
                    line,
                )
            });
            continue;
        }

        if let Some(c) = r_java_ver().captures(line) {
            let need = c[1].parse::<u32>().unwrap_or(0).saturating_sub(44);
            let have = c[2].parse::<u32>().unwrap_or(0).saturating_sub(44);
            b.add("java_version".into(), || {
                finding(
                    "java_version",
                    Severity::Error,
                    format!("Нужна Java {need}, а запущена Java {have}"),
                    "Сервер или мод собран под более новую Java.".into(),
                    "Установи нужную версию Java и укажи путь к ней на вкладке «Запуск».",
                    line,
                )
            });
            continue;
        }

        if let Some(c) = r_cant_keep_up().captures(line) {
            let ms = c[1].to_string();
            b.add("lag".into(), || {
                finding(
                    "lag",
                    Severity::Warn,
                    "Сервер не успевает (Can't keep up)".into(),
                    format!("Отставание доходило до {ms} мс."),
                    "Если это часто: уменьши view/simulation-distance, предгенерируй мир (Chunky), найди тяжёлые моды через spark.",
                    line,
                )
            });
            continue;
        }

        if r_port().is_match(line) {
            b.add("port".into(), || {
                finding(
                    "port",
                    Severity::Error,
                    "Порт сервера уже занят".into(),
                    "Скорее всего, уже запущен другой сервер или старый процесс не закрылся.".into(),
                    "Останови другой сервер или смени server-port в server.properties.",
                    line,
                )
            });
            continue;
        }
        if r_oom().is_match(line) {
            b.add("oom".into(), || {
                finding(
                    "oom",
                    Severity::Error,
                    "Не хватило памяти (OutOfMemoryError)".into(),
                    "Java упёрлась в лимит -Xmx.".into(),
                    "Увеличь память на вкладке «Запуск» или найди мод с утечкой памяти (spark heapsummary).",
                    line,
                )
            });
            continue;
        }
        if r_watchdog().is_match(line) {
            b.add("watchdog".into(), || {
                finding(
                    "watchdog",
                    Severity::Error,
                    "Сервер завис (сработал watchdog)".into(),
                    "Один тик длился слишком долго, и сервер остановил сам себя.".into(),
                    "Смотри стектрейс ниже — в нём обычно виден мод, на котором всё застряло.",
                    line,
                )
            });
            if let Some(m) = first_mod_frame(&lines, i + 1, 60, b.idx) {
                b.add_culprit("watchdog", culprit(m, Confidence::Low));
            }
            continue;
        }
        if let Some(c) = r_chunk().captures(line) {
            let _ = c;
            b.add("chunk".into(), || {
                finding(
                    "chunk",
                    Severity::Warn,
                    "Повреждённые чанки в мире".into(),
                    "Сервер не смог прочитать часть мира.".into(),
                    "Если ошибок много — откатись на снапшот до появления проблемы.",
                    line,
                )
            });
            continue;
        }
        if let Some(c) = r_config().captures(line) {
            let file = c.get(2).or(c.get(3)).map(|x| x.as_str().to_string()).unwrap_or_default();
            b.add(format!("config:{file}"), || {
                finding(
                    "config",
                    Severity::Warn,
                    if file.is_empty() { "Ошибка в файле настроек мода".into() } else { format!("Ошибка в файле настроек {file}") },
                    "Мод не смог прочитать свой конфиг.".into(),
                    "Удали или переименуй этот файл — мод создаст новый с настройками по умолчанию.",
                    line,
                )
            });
            continue;
        }
        if let Some(c) = r_invalid_jar().captures(line) {
            let file = c.get(1).map(|x| x.as_str().to_string()).unwrap_or_default();
            b.add("invalid_jar".into(), || {
                finding(
                    "invalid_jar",
                    Severity::Error,
                    "Повреждённый jar-файл".into(),
                    if file.is_empty() { "Один из файлов не открывается как архив.".into() } else { format!("Файл {file} повреждён.") },
                    "Скачай этот мод заново.",
                    line,
                )
            });
            if let Some(m) = b.idx.find(&file) {
                b.add_culprit("invalid_jar", culprit(m, Confidence::High));
            }
            continue;
        }
        if r_eula().is_match(line) {
            b.add("eula".into(), || {
                finding("eula", Severity::Error, "Не принята EULA".into(), "В eula.txt должно быть eula=true.".into(), "Нажми «Запустить» — менеджер предложит принять EULA.", line)
            });
            continue;
        }
        if r_tick_loop().is_match(line) {
            b.add("tick_crash".into(), || {
                finding(
                    "tick_crash",
                    Severity::Error,
                    "Сервер упал во время работы".into(),
                    "Исключение в основном цикле сервера.".into(),
                    "Смотри «Сжатый лог» и виновников ниже. Убирать контентный мод опасно — его блоки исчезнут из мира.",
                    line,
                )
            });
            if let Some(m) = first_mod_frame(&lines, i + 1, 60, b.idx) {
                b.add_culprit("tick_crash", culprit(m, Confidence::Low));
            }
            continue;
        }

        // --- краш-репорт
        if let Some(c) = r_description().captures(line) {
            let d = c[1].to_string();
            b.add("crash_report".into(), || {
                finding(
                    "crash_report",
                    Severity::Error,
                    format!("Краш-репорт: {d}"),
                    String::new(),
                    "Ниже — моды, которые упоминаются в отчёте.",
                    line,
                )
            });
            if let Some(m) = first_mod_frame(&lines, i + 1, 80, b.idx) {
                b.add_culprit("crash_report", culprit(m, Confidence::Low));
            }
            continue;
        }
        if let Some(c) = r_suspected().captures(line) {
            let list = c[1].to_string();
            if list.trim().eq_ignore_ascii_case("none") || list.contains("NONE") {
                continue;
            }
            b.add("crash_report".into(), || {
                finding("crash_report", Severity::Error, "Краш-репорт".into(), String::new(), "", line)
            });
            for it in r_suspected_item().captures_iter(&list) {
                if let Some(m) = b.idx.find(&it[2]).or_else(|| b.idx.find(&it[1])) {
                    b.add_culprit("crash_report", culprit(m, Confidence::Medium));
                }
            }
            continue;
        }
    }

    let mut out: Vec<Finding> = b.map.into_values().collect();
    for f in &mut out {
        f.culprits.sort_by(|a, b| b.confidence.cmp(&a.confidence));
    }
    out.sort_by(|a, b| b.severity.cmp(&a.severity).then(b.culprits.len().cmp(&a.culprits.len())));
    out
}

fn first_mod_frame<'a>(lines: &[&str], from: usize, max: usize, idx: &ModIndex<'a>) -> Option<&'a ModInfo> {
    for l in lines.iter().skip(from).take(max) {
        if let Some(c) = r_frame().captures(l) {
            if let Some(m) = idx.by_class(&c[1]) {
                return Some(m);
            }
        } else if !l.trim().is_empty() && !l.trim_start().starts_with("Caused by") && !l.contains("...") && !l.trim_start().starts_with("at ") {
            // стектрейс кончился
            if l.starts_with(char::is_alphabetic) && !l.contains("Exception") && !l.contains("Error") {
                break;
            }
        }
    }
    None
}

// ===================================================================== сжатый лог

/// Оставляет только важное. Повторы склеиваются («×12»), стектрейсы обрезаются.
pub fn compress(text: &str, header: &str) -> String {
    const MAX_FRAMES: usize = 8;
    const MAX_LINES: usize = 400;
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = vec![header.to_string()];
    let mut seen: HashMap<String, usize> = HashMap::new(); // нормализованная строка → индекс в out
    let mut counts: HashMap<usize, usize> = HashMap::new();
    let keep_info = re(r"(?i)(Starting minecraft server version|Loading Minecraft|ModLauncher running|Forge mod loading|NeoForge|Fabric Loader|Loading \d+ mods|Java is|Java version|java\.version|Operating System|Done \(|Stopping the server|Preparing level|Crash report saved|Suspected Mods|Description:|Time:|Minecraft Version|Memory:|JVM Flags|Is Modded)");
    let ts = re(r"^\[[^\]]*\]\s*");
    let digits = re(r"\d+");

    let mut i = 0;
    let mut in_section_skip = false;
    while i < lines.len() && out.len() < MAX_LINES {
        let l = lines[i].trim_end();
        // в краш-репорте длинные служебные секции — список модов, системные детали — пропускаем
        if l.starts_with("-- ") && (l.contains("System Details") || l.contains("Mod List") || l.contains("Affected level")) {
            in_section_skip = true;
            out.push(format!("{l} [пропущено]"));
            i += 1;
            continue;
        }
        if in_section_skip {
            if l.starts_with("-- ") || l.starts_with("---- ") {
                in_section_skip = false;
            } else {
                if keep_info.is_match(l) {
                    out.push(l.to_string());
                }
                i += 1;
                continue;
            }
        }

        let lvl = crate::process::detect_level(l);
        let is_exc = r_exception_head().is_match(l.trim_start_matches(|c: char| c == '\t' || c == ' '));
        let important = lvl == crate::process::Level::Error
            || lvl == crate::process::Level::Warn
            || is_exc
            || keep_info.is_match(l)
            || l.starts_with("---- ")
            || l.starts_with("-- ");

        if is_exc || (lvl == crate::process::Level::Error && !l.starts_with('\t') && !l.trim_start().starts_with("at ")) {
            // заголовок исключения + несколько кадров
            out.push(l.to_string());
            let mut j = i + 1;
            let mut frames = 0;
            let mut skipped = 0;
            while j < lines.len() {
                let s = lines[j].trim_end();
                let t = s.trim_start();
                if t.starts_with("at ") {
                    if frames < MAX_FRAMES {
                        out.push(s.to_string());
                    } else {
                        skipped += 1;
                    }
                    frames += 1;
                } else if t.starts_with("Caused by:") {
                    if skipped > 0 {
                        out.push(format!("\t… ещё {skipped} строк"));
                        skipped = 0;
                    }
                    out.push(s.to_string());
                    frames = 0;
                } else if t.starts_with("...") {
                    // «... 12 more» — пропускаем
                } else {
                    break;
                }
                j += 1;
            }
            if skipped > 0 {
                out.push(format!("\t… ещё {skipped} строк"));
            }
            i = j;
            continue;
        }

        if important {
            // повторы склеиваем по «нормализованной» форме (без времени и чисел)
            let norm = digits.replace_all(&ts.replace(l, ""), "#").to_string();
            if let Some(&pos) = seen.get(&norm) {
                *counts.entry(pos).or_insert(1) += 1;
            } else {
                seen.insert(norm, out.len());
                out.push(l.to_string());
            }
        }
        i += 1;
    }
    for (pos, n) in counts {
        if n > 1 {
            out[pos] = format!("{}  ×{n}", out[pos]);
        }
    }
    if out.len() >= MAX_LINES {
        out.push(format!("… обрезано до {MAX_LINES} строк"));
    }
    out.join("\n")
}

// ===================================================================== источники

/// Самый свежий краш-репорт сервера.
pub fn latest_crash_report(server_dir: &Path) -> Option<PathBuf> {
    fs::read_dir(server_dir.join("crash-reports"))
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().map(|x| x == "txt").unwrap_or(false))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

pub fn crash_report_since(server_dir: &Path, since: std::time::SystemTime) -> Option<PathBuf> {
    let p = latest_crash_report(server_dir)?;
    let m = fs::metadata(&p).ok()?.modified().ok()?;
    (m >= since).then_some(p)
}

/// Полный анализ: краш-репорт и/или лог.
pub fn run(source: &str, source_path: &str, text: &str, mods: Option<&ModReport>, header: &str) -> Analysis {
    let idx = ModIndex::new(mods);
    let findings = analyze_text(text, &idx);
    let mut errors = 0;
    let mut warnings = 0;
    for l in text.lines() {
        match crate::process::detect_level(l) {
            crate::process::Level::Error => errors += 1,
            crate::process::Level::Warn => warnings += 1,
            _ => {}
        }
    }
    let lag = text.lines().filter(|l| r_cant_keep_up().is_match(l)).count();
    Analysis {
        source: source.into(),
        source_path: source_path.into(),
        compressed: compress(text, header),
        total_lines: text.lines().count(),
        findings,
        errors,
        warnings,
        lag_warnings: lag,
    }
}

/// Кого можно отправить в карантин по результатам анализа.
/// `allow_content` — разрешить убирать не только клиентские моды (рискованно для мира).
pub fn quarantine_candidates(findings: &[Finding], allow_content: bool, min: Confidence) -> Vec<(Culprit, String)> {
    let mut out: Vec<(Culprit, String)> = Vec::new();
    for f in findings {
        // отсутствующую зависимость карантином не исправить
        if f.code == "missing_dep" || f.code == "duplicate" || f.code == "eula" || f.code == "port" || f.code == "oom" || f.code == "java_version" {
            continue;
        }
        for c in &f.culprits {
            if c.confidence < min {
                continue;
            }
            let safe = c.client_only || f.code == "client_class";
            if !safe && !allow_content {
                continue;
            }
            if !out.iter().any(|(x, _)| x.file == c.file) {
                out.push((c.clone(), f.title.clone()));
            }
        }
    }
    out
}

/// Собирает текст для анализа: краш-репорт (если появился после запуска) + консоль.
pub fn gather(server_dir: &Path, console: &str, since: Option<std::time::SystemTime>) -> (String, String, String) {
    let crash = match since {
        Some(t) => crash_report_since(server_dir, t),
        None => latest_crash_report(server_dir),
    };
    let mut text = String::new();
    let mut source = "console".to_string();
    let mut path = String::new();
    if let Some(p) = &crash {
        if let Ok(c) = fs::read_to_string(p) {
            text.push_str(&c);
            text.push('\n');
            source = "crash+console".into();
            path = p.to_string_lossy().into();
        }
    }
    text.push_str(console);
    (text, source, path)
}

/// Автокарантин после падения. Возвращает анализ и список перенесённых модов.
pub fn auto_quarantine(
    server: &crate::config::ServerEntry,
    text: &str,
    reached_running: bool,
) -> crate::Result<(Vec<Finding>, Vec<crate::mods::MovedMod>)> {
    let dir = server.dir();
    let report = crate::mods::list(&dir, server.core.mod_loader(), &server.mods);
    let idx = ModIndex::new(Some(&report));
    let findings = analyze_text(text, &idx);
    // упал уже во время игры — контентные моды не трогаем никогда (иначе пропадут блоки)
    let allow_content = server.mods.quarantine_content && !reached_running;
    let keep: Vec<String> = server.mods.never_disable.iter().map(|x| x.to_lowercase()).collect();
    let mut moved = Vec::new();
    for (c, why) in quarantine_candidates(&findings, allow_content, Confidence::Medium) {
        let Some(m) = report.mods.iter().find(|m| m.file == c.file && m.enabled) else { continue };
        if m.meta.ids.iter().any(|id| keep.contains(&id.to_lowercase())) {
            continue;
        }
        crate::mods::set_enabled(&dir, &m.file, false, "crash", &why)?;
        moved.push(crate::mods::MovedMod { file: m.file.clone(), name: c.name.clone(), reason: why });
    }
    Ok((findings, moved))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::{analyze as analyze_mods, ModInfo};

    fn fake_mod(file: &str, id: &str, pkg: &str, client: bool) -> ModInfo {
        let mut meta = crate::mods::read_meta(Path::new("/nonexistent"));
        meta.ids = vec![id.into()];
        meta.name = id.to_uppercase();
        meta.packages = vec![pkg.into()];
        if client {
            meta.client = crate::mods::ClientConfidence::Definite;
        }
        ModInfo {
            file: file.into(),
            enabled: true,
            size: 0,
            id: id.into(),
            meta,
            disabled_reason: None,
            disabled_detail: None,
            required_by: vec![],
            missing_deps: vec![],
            problems: vec![],
        }
    }

    fn report() -> ModReport {
        let mut v = vec![
            fake_mod("create.jar", "create", "com.simibubi.create", false),
            fake_mod("zoomy.jar", "zoomy", "dev.zoomy.client", true),
            fake_mod("fancy.jar", "fancymenu", "de.keksuccino.fancymenu", false),
        ];
        analyze_mods(&mut v, None)
    }

    #[test]
    fn client_class_with_frame() {
        let r = report();
        let idx = ModIndex::new(Some(&r));
        let log = "[main/ERROR]: Failed\n\
java.lang.RuntimeException: Attempted to load class net/minecraft/client/gui/screens/Screen for invalid dist DEDICATED_SERVER\n\
\tat net.neoforged.fml.loading.RuntimeDistCleaner.processClassWithFlags(RuntimeDistCleaner.java:57)\n\
\tat dev.zoomy.client.ZoomHandler.<clinit>(ZoomHandler.java:12)\n";
        let f = analyze_text(log, &idx);
        let c = f.iter().find(|x| x.code == "client_class").unwrap();
        assert_eq!(c.culprits[0].file, "zoomy.jar");
        assert_eq!(c.culprits[0].confidence, Confidence::High);
        let q = quarantine_candidates(&f, false, Confidence::Medium);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn neo_missing_and_mixin_and_suspected() {
        let r = report();
        let idx = ModIndex::new(Some(&r));
        let log = "\tMod ID: 'flywheel', Requested by: 'create', Expected range: '[1.0,)', Actual version: '[MISSING]'\n\
[main/ERROR] [mixin/]: Mixin apply for mod fancymenu failed fancymenu.mixins.json:MixinScreen\n\
Description: Exception in server tick loop\n\
Suspected Mods: Create (create), Minecraft (minecraft)\n";
        let f = analyze_text(log, &idx);
        assert!(f.iter().any(|x| x.code == "missing_dep" && x.title.contains("flywheel")));
        let mx = f.iter().find(|x| x.code == "mixin").unwrap();
        assert_eq!(mx.culprits[0].file, "fancy.jar");
        let cr = f.iter().find(|x| x.code == "crash_report").unwrap();
        assert_eq!(cr.culprits[0].file, "create.jar");
        // Create — контентный мод: без allow_content в карантин не идёт
        let q = quarantine_candidates(&f, false, Confidence::Medium);
        assert!(q.iter().all(|(c, _)| c.file != "create.jar"));
        let q2 = quarantine_candidates(&f, true, Confidence::Medium);
        assert!(q2.iter().any(|(c, _)| c.file == "create.jar"));
    }

    #[test]
    fn misc_rules() {
        let idx = ModIndex::new(None);
        let log = "[Server thread/WARN]: Can't keep up! Is the server overloaded? Running 5012ms or 100 ticks behind\n\
java.lang.UnsupportedClassVersionError: x has been compiled by a more recent version of the Java Runtime (class file version 65.0), this version of the Java Runtime only recognizes class file versions up to 61.0\n\
**** FAILED TO BIND TO PORT!\n";
        let f = analyze_text(log, &idx);
        let jv = f.iter().find(|x| x.code == "java_version").unwrap();
        assert!(jv.title.contains("Java 21") && jv.title.contains("Java 17"));
        assert!(f.iter().any(|x| x.code == "lag"));
        assert!(f.iter().any(|x| x.code == "port"));
    }

    #[test]
    fn quarantine_moves_client_culprit() {
        use std::io::Write;
        let t = tempfile::tempdir().unwrap();
        let mods = t.path().join("mods");
        fs::create_dir_all(&mods).unwrap();
        let mk = |name: &str, toml: &str, class: &str| {
            let f = fs::File::create(mods.join(name)).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("META-INF/neoforge.mods.toml", o).unwrap();
            z.write_all(toml.as_bytes()).unwrap();
            for n in 0..3 {
                z.start_file(format!("{class}/C{n}.class"), o).unwrap();
                z.write_all(b"x").unwrap();
            }
            z.finish().unwrap();
        };
        mk("hud.jar", "[[mods]]\nmodId=\"hud\"\n", "dev/hud/client");
        mk("create.jar", "[[mods]]\nmodId=\"create\"\n", "com/simibubi/create");
        let s = crate::config::ServerEntry {
            path: t.path().to_string_lossy().into(),
            core: crate::detect::CoreKind::NeoForge,
            ..Default::default()
        };
        let log = "java.lang.RuntimeException: Attempted to load class net/minecraft/client/Minecraft for invalid dist DEDICATED_SERVER\n\tat dev.hud.client.C0.init(C0.java:1)\n";
        let (_f, moved) = auto_quarantine(&s, log, false).unwrap();
        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].file, "hud.jar");
        assert!(t.path().join("mods_disabled/hud.jar").exists());
        assert!(mods.join("create.jar").exists());
        let r = crate::mods::read_reasons(&t.path().join("mods_disabled"));
        assert_eq!(r["hud.jar"].reason, "crash");
    }

    #[test]
    fn compress_trims_and_merges() {
        let mut log = String::from("[10:00:00] [main/INFO]: Starting minecraft server version 1.21.1\n");
        for n in 0..500 {
            log.push_str(&format!("[10:00:01] [main/INFO]: Loading thing {n}\n"));
        }
        for n in 0..5 {
            log.push_str(&format!("[10:00:0{n}] [main/WARN]: Missing texture {n}\n"));
        }
        log.push_str("java.lang.NullPointerException: boom\n");
        for n in 0..30 {
            log.push_str(&format!("\tat com.foo.Bar.m{n}(Bar.java:{n})\n"));
        }
        log.push_str("Caused by: java.lang.IllegalStateException: inner\n\tat com.foo.Baz.x(Baz.java:1)\n\t... 12 more\n");
        let c = compress(&log, "== test ==");
        assert!(c.contains("Starting minecraft server version 1.21.1"));
        assert!(!c.contains("Loading thing 42"));
        assert!(c.contains("Missing texture") && c.contains("×5"));
        assert!(c.contains("NullPointerException") && c.contains("… ещё 22 строк"));
        assert!(c.contains("Caused by: java.lang.IllegalStateException"));
        assert!(c.lines().count() < 30, "{c}");
    }
}
