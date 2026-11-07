//! TPS и MSPT: какую команду отправить серверу и как разобрать ответ.
//!
//! - NeoForge / Forge: `neoforge tps` / `forge tps` → «Overall: 20.000 TPS (1.234 ms/tick)»
//! - Paper и форки: `tps` → «TPS from last 1m, 5m, 15m: 20.0, 20.0, 20.0», `mspt`
//! - spark (любой загрузчик): `spark tps` → TPS за 5с…15м и длительности тиков
//!
//! Строки ответа перехватываются (`ProcessManager::query`) и не засоряют консоль.

use regex::Regex;
use serde::Serialize;

use crate::detect::CoreKind;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Tps {
    pub tps: f32,
    /// Миллисекунд на тик (средн./медиана), если известно
    pub mspt: Option<f32>,
    /// Пиковая длительность тика (95-й перцентиль или максимум), если известно
    pub mspt_peak: Option<f32>,
    /// Какой командой получено
    pub via: String,
    /// мс с начала эпохи
    pub ts: i64,
}

/// Регулярка строк ответа (для перехвата). Широкая, но действует только 2–3 секунды.
pub const RESPONSE_PATTERN: &str = r"(?i)(\[⚡\]|TPS \(|ms/tick|Mean tick time|TPS from last|tick times|Tick durations|^\s*(\[[^\]]*\]\s*)*(\[[^\]]*\]:\s*)?[\s*◴⚡]*\*?\d+(\.\d+)?[\s,/;*◴]+\*?\d|Unknown or incomplete command|<--\[HERE\]|Unknown command)";

/// Команды по порядку предпочтения.
pub fn commands(core: CoreKind, has_spark: bool) -> Vec<&'static str> {
    let mut v = Vec::new();
    if has_spark {
        v.push("spark tps");
    }
    match core {
        CoreKind::NeoForge | CoreKind::Youer => v.push("neoforge tps"),
        CoreKind::Forge | CoreKind::Mohist => v.push("forge tps"),
        CoreKind::Arclight => {
            v.push("neoforge tps");
            v.push("forge tps");
        }
        CoreKind::Paper | CoreKind::Purpur | CoreKind::Folia | CoreKind::Spigot => {
            v.push("tps");
        }
        _ => {}
    }
    // гибриды Bukkit тоже знают `tps`
    if matches!(core, CoreKind::Youer | CoreKind::Mohist | CoreKind::Arclight) {
        v.push("tps");
    }
    v
}

fn num(s: &str) -> Option<f32> {
    s.trim().trim_start_matches('*').replace(',', ".").parse().ok()
}

fn strip_prefix(l: &str) -> &str {
    // «[12:00:00] [Server thread/INFO] [minecraft/MinecraftServer]: текст» → «текст»
    let s = match l.rfind("]: ") {
        Some(i) => &l[i + 3..],
        None => l.trim(),
    };
    // spark начинает каждую строку с «[⚡]»
    s.trim_start().trim_start_matches("[⚡]").trim()
}

/// Команда не поддерживается сервером.
pub fn is_unknown(lines: &[String]) -> bool {
    lines.iter().any(|l| l.contains("Unknown or incomplete command") || l.contains("Unknown command"))
        && !lines.iter().any(|l| l.contains("TPS"))
}

pub fn parse(lines: &[String], via: &str) -> Option<Tps> {
    let now = chrono::Utc::now().timestamp_millis();
    let body: Vec<&str> = lines.iter().map(|l| strip_prefix(l)).collect();

    // NeoForge / Forge 1.20.5+: «Overall: 20.000 TPS (0.143 ms/tick)»
    let re_neo = Regex::new(r"(?i)^(.+?):\s*([\d.,]+)\s*TPS\s*\(([\d.,]+)\s*ms/tick\)").unwrap();
    let mut overall: Option<(f32, f32)> = None;
    let mut dims_ms = 0.0f32;
    let mut dims_min_tps: Option<f32> = None;
    for b in &body {
        if let Some(c) = re_neo.captures(b) {
            let (tps, ms) = (num(&c[2])?, num(&c[3])?);
            if c[1].trim().eq_ignore_ascii_case("overall") {
                overall = Some((tps, ms));
            } else {
                dims_ms += ms;
                dims_min_tps = Some(dims_min_tps.map_or(tps, |m: f32| m.min(tps)));
            }
        }
    }
    if let Some((tps, ms)) = overall {
        // у NeoForge «Overall» мс бывает 0 — тогда суммируем измерения
        let ms = if ms > 0.0 { ms } else { dims_ms };
        return Some(Tps { tps, mspt: Some(ms), mspt_peak: None, via: via.into(), ts: now });
    }
    if let Some(tps) = dims_min_tps {
        return Some(Tps { tps, mspt: Some(dims_ms), mspt_peak: None, via: via.into(), ts: now });
    }

    // Старый Forge: «Overall : Mean tick time: 12.345 ms. Mean TPS: 20.000»
    let re_forge = Regex::new(r"(?i)Overall\s*:\s*Mean tick time:\s*([\d.,]+)\s*ms\.\s*Mean TPS:\s*([\d.,]+)").unwrap();
    for b in &body {
        if let Some(c) = re_forge.captures(b) {
            return Some(Tps { tps: num(&c[2])?, mspt: num(&c[1]), mspt_peak: None, via: via.into(), ts: now });
        }
    }

    // Paper / Bukkit: «TPS from last 1m, 5m, 15m: 20.0, 20.0, 20.0» (+ spark: значения строкой ниже)
    let re_nums = Regex::new(r"\*?\d+(?:[.,]\d+)?").unwrap();
    for (i, b) in body.iter().enumerate() {
        if b.contains("TPS from last") {
            let after = b.split_once(':').map(|x| x.1).unwrap_or("");
            let mut vals: Vec<f32> = re_nums.find_iter(after).filter_map(|m| num(m.as_str())).collect();
            if vals.is_empty() {
                // значения — в первой следующей непустой строке, и это должна быть строка из чисел
                if let Some(next) = body.iter().skip(i + 1).find(|x| !x.trim().is_empty()) {
                    if next.chars().all(|c| c.is_ascii_digit() || " .,*".contains(c)) {
                        vals = re_nums.find_iter(next).filter_map(|m| num(m.as_str())).collect();
                    }
                }
            }
            // spark: 5s, 10s, 1m, … — берём 1m (третье), Paper: 1m — первое
            let tps = if via.starts_with("spark") { vals.get(2).or(vals.first()) } else { vals.first() };
            let Some(&tps) = tps else { continue };
            let mut out = Tps { tps: tps.min(20.0), mspt: None, mspt_peak: None, via: via.into(), ts: now };
            // spark: «Tick durations (min/med/95%ile/max ms) from last 10s, 1m:» → «0.4/0.6/1.2/5.0; …»
            if let Some(j) = body.iter().position(|x| x.contains("Tick durations") || x.contains("tick times")) {
                if let Some(line) = body.iter().skip(j + 1).find(|x| !x.trim().is_empty()) {
                    let first = line.split([';', ',']).next().unwrap_or("");
                    let parts: Vec<f32> = re_nums.find_iter(first).filter_map(|m| num(m.as_str())).collect();
                    if body[j].contains("Tick durations") && parts.len() >= 4 {
                        out.mspt = Some(parts[1]);
                        out.mspt_peak = Some(parts[2]);
                    } else if parts.len() >= 3 {
                        // Paper mspt: avg/min/max
                        out.mspt = Some(parts[0]);
                        out.mspt_peak = Some(parts[2]);
                    }
                }
            }
            return Some(out);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Vec<String> {
        s.lines().map(String::from).collect()
    }

    #[test]
    fn neoforge_real_output() {
        let l = v("[17:19:48] [Server thread/INFO] [minecraft/MinecraftServer]: Overworld: 19.500 TPS (12.143 ms/tick)\n\
[17:19:48] [Server thread/INFO] [minecraft/MinecraftServer]: The Nether: 20.000 TPS (0.501 ms/tick)\n\
[17:19:48] [Server thread/INFO] [minecraft/MinecraftServer]: Overall: 19.500 TPS (0.000 ms/tick)");
        let t = parse(&l, "neoforge tps").unwrap();
        assert_eq!(t.tps, 19.5);
        assert!((t.mspt.unwrap() - 12.644).abs() < 0.01);
        let re = Regex::new(RESPONSE_PATTERN).unwrap();
        assert!(l.iter().all(|x| re.is_match(x)));
    }

    #[test]
    fn paper_and_spark() {
        let p = parse(&v("[12:00:00 INFO]: TPS from last 1m, 5m, 15m: *20.0, 19.98, 19.7"), "tps").unwrap();
        assert_eq!(p.tps, 20.0);
        let s = parse(
            &v("[12:00:00] [Server thread/INFO]: TPS from last 5s, 10s, 1m, 5m, 15m:\n\
[12:00:00] [Server thread/INFO]:  20.0, 20.0, 18.4, 19.9, 20.0\n\
[12:00:00] [Server thread/INFO]: Tick durations (min/med/95%ile/max ms) from last 10s, 1m:\n\
[12:00:00] [Server thread/INFO]:  0.4/6.2/41.0/120.3;  0.3/5.9/38.2/250.1"),
            "spark tps",
        )
        .unwrap();
        assert_eq!(s.tps, 18.4);
        assert_eq!(s.mspt, Some(6.2));
        assert_eq!(s.mspt_peak, Some(41.0));
    }

    /// Настоящий вывод spark 1.10 на NeoForge 1.21.1 (из живого запуска).
    #[test]
    fn spark_real_output() {
        let raw = "[20:49:47] [spark-worker-pool-1-thread-3/INFO] [minecraft/MinecraftServer]: [⚡] TPS from last 5s, 10s, 1m, 5m, 15m:\n\
[20:49:47] [spark-worker-pool-1-thread-3/INFO] [minecraft/MinecraftServer]: [⚡]  19.99, 20.0, *20.0, *20.0, *20.0\n\
[20:49:47] [spark-worker-pool-1-thread-3/INFO] [minecraft/MinecraftServer]: [⚡] \n\
[20:49:47] [spark-worker-pool-1-thread-3/INFO] [minecraft/MinecraftServer]: [⚡] Tick durations (min/med/95%ile/max ms) from last 10s, 1m:\n\
[20:49:47] [spark-worker-pool-1-thread-3/INFO] [minecraft/MinecraftServer]: [⚡]  0.2/0.9/33.1/63.9;  0.2/1.0/38.3/63.9\n\
[20:49:47] [spark-worker-pool-1-thread-3/INFO] [minecraft/MinecraftServer]: [⚡] \n\
[20:49:47] [spark-worker-pool-1-thread-3/INFO] [minecraft/MinecraftServer]: [⚡] CPU usage from last 10s, 1m, 15m:\n\
[20:49:47] [spark-worker-pool-1-thread-3/INFO] [minecraft/MinecraftServer]: [⚡]  76%, 88%, 88%  (system)\n\
[20:49:47] [spark-worker-pool-1-thread-3/INFO] [minecraft/MinecraftServer]: [⚡]  11%, 34%, 34%  (process)";
        let lines = v(raw);
        let re = Regex::new(RESPONSE_PATTERN).unwrap();
        assert!(lines.iter().all(|l| re.is_match(l)), "не все строки spark перехватываются");
        let t = parse(&lines, "spark tps").unwrap();
        assert_eq!(t.tps, 20.0);
        assert_eq!(t.mspt, Some(0.9));
        assert_eq!(t.mspt_peak, Some(33.1));
    }

    #[test]
    fn unknown_and_normal_lines_not_caught() {
        assert!(is_unknown(&v("[x]: Unknown or incomplete command, see below for error\n[x]: tps<--[HERE]")));
        let re = Regex::new(RESPONSE_PATTERN).unwrap();
        assert!(!re.is_match("[12:00:00] [Server thread/INFO]: Steve joined the game"));
        assert!(!re.is_match("[12:00:00] [Server thread/INFO]: <Steve> привет"));
    }

    #[test]
    fn commands_by_core() {
        assert_eq!(commands(CoreKind::Youer, false), vec!["neoforge tps", "tps"]);
        assert_eq!(commands(CoreKind::Fabric, true), vec!["spark tps"]);
        assert!(commands(CoreKind::Vanilla, false).is_empty());
    }
}
