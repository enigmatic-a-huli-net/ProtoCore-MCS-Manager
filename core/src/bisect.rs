//! Автоперебор модов: на тестовой копии сервера ищет мод, из-за которого сервер падает.
//!
//! Тестовая копия:
//! - лежит рядом с сервером (`<папка>.pcm-test`), чтобы работали жёсткие ссылки;
//! - jar-файлы и библиотеки — жёсткие ссылки (не занимают места), конфиги — настоящие копии
//!   (тестовый сервер может их менять);
//! - свой мир (`pcm-test-world`) и свой порт — настоящий мир не трогается.
//!
//! Режимы:
//! - **группы** (по умолчанию по 5): моды добавляются группами вместе с зависимостями;
//!   если группа уронила сервер — моды группы проверяются по одному;
//! - **бинарный**: делим подозреваемых пополам — быстрее при большом числе модов.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::mods::{ModInfo, ModReport, Side};
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BisectSettings {
    /// "batch" или "binary"
    pub mode: String,
    pub batch: usize,
    /// Сколько ждать запуска, сек (дольше — считаем, что завис)
    pub timeout_sec: u32,
    pub memory_mb: u32,
    /// Искать все проблемные моды, а не только первый
    pub find_all: bool,
    /// Оставить тестовую копию после проверки
    pub keep_copy: bool,
}

impl Default for BisectSettings {
    fn default() -> Self {
        Self { mode: "batch".into(), batch: 5, timeout_sec: 300, memory_mb: 6144, find_all: false, keep_copy: false }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", content = "detail", rename_all = "lowercase")]
pub enum Outcome {
    Ok,
    Crash(String),
    Hang,
    Error(String),
}

impl Outcome {
    pub fn ok(&self) -> bool {
        matches!(self, Outcome::Ok)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub n: usize,
    pub label: String,
    pub mods: usize,
    pub outcome: Outcome,
    pub seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Culprit {
    pub file: String,
    pub name: String,
    /// почему сервер упал (заголовок из диагностики)
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Report {
    pub steps: Vec<Step>,
    pub culprits: Vec<Culprit>,
    /// Упало только вместе — моды этой группы конфликтуют друг с другом
    pub group: Vec<String>,
    /// Сервер не запускается даже без модов — проблема не в модах
    pub baseline_failed: bool,
    /// Со всеми модами сервер запустился — краш не повторился
    pub all_ok: bool,
    pub cancelled: bool,
    pub message: String,
}

/// Кто умеет запускать тестовый сервер с заданным набором модов.
pub trait Runner {
    fn run(&mut self, label: &str, files: &BTreeSet<String>) -> Outcome;
    fn cancelled(&self) -> bool {
        false
    }
}

// ===================================================================== зависимости

pub struct Graph {
    pub all: Vec<String>,
    names: HashMap<String, String>,
    deps: HashMap<String, Vec<String>>,
}

impl Graph {
    /// Только включённые моды. Зависимости — обязательные, серверные, которые есть среди модов.
    pub fn from_report(r: &ModReport) -> Self {
        let enabled: Vec<&ModInfo> = r.mods.iter().filter(|m| m.enabled).collect();
        let mut by_id: HashMap<String, String> = HashMap::new();
        for m in &enabled {
            for id in m.meta.ids.iter().chain(m.meta.provides.iter()) {
                by_id.entry(id.to_lowercase()).or_insert_with(|| m.file.clone());
            }
        }
        let mut deps = HashMap::new();
        let mut names = HashMap::new();
        for m in &enabled {
            let d: Vec<String> = m
                .meta
                .deps
                .iter()
                .filter(|d| d.required && d.side != Side::Client)
                .filter_map(|d| by_id.get(&d.id.to_lowercase()).cloned())
                .filter(|f| f != &m.file)
                .collect();
            deps.insert(m.file.clone(), d);
            names.insert(m.file.clone(), if m.meta.name.is_empty() { m.file.clone() } else { m.meta.name.clone() });
        }
        let mut all: Vec<String> = enabled.iter().map(|m| m.file.clone()).collect();
        all.sort();
        Self { all, names, deps }
    }

    pub fn name(&self, f: &str) -> String {
        self.names.get(f).cloned().unwrap_or_else(|| f.to_string())
    }

    /// Набор вместе со всеми зависимостями.
    pub fn closure(&self, set: impl IntoIterator<Item = String>) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut stack: Vec<String> = set.into_iter().collect();
        while let Some(f) = stack.pop() {
            if out.insert(f.clone()) {
                for d in self.deps.get(&f).into_iter().flatten() {
                    stack.push(d.clone());
                }
            }
        }
        out
    }

    /// Моды, которые (прямо или через другие) требуют `f`.
    pub fn dependents(&self, f: &str) -> BTreeSet<String> {
        self.all.iter().filter(|m| m.as_str() != f && self.closure([(*m).clone()]).contains(f)).cloned().collect()
    }
}

// ===================================================================== алгоритм

struct Ctx<'a, R: Runner> {
    g: &'a Graph,
    r: &'a mut R,
    rep: Report,
    on_step: &'a mut dyn FnMut(&Step),
}

impl<R: Runner> Ctx<'_, R> {
    fn test(&mut self, label: String, set: &BTreeSet<String>) -> Option<Outcome> {
        if self.r.cancelled() {
            self.rep.cancelled = true;
            return None;
        }
        let t0 = std::time::Instant::now();
        let out = self.r.run(&label, set);
        let step = Step { n: self.rep.steps.len() + 1, label, mods: set.len(), outcome: out.clone(), seconds: t0.elapsed().as_secs() };
        (self.on_step)(&step);
        self.rep.steps.push(step);
        Some(out)
    }

    fn reason(o: &Outcome) -> String {
        match o {
            Outcome::Crash(s) => s.clone(),
            Outcome::Hang => "сервер завис при запуске".into(),
            Outcome::Error(s) => s.clone(),
            Outcome::Ok => String::new(),
        }
    }

    /// Найти виновника среди `cands` (каждый проверяется вместе с `good` и своими зависимостями).
    fn narrow(&mut self, good: &BTreeSet<String>, cands: &[String]) -> Option<Option<(String, Outcome)>> {
        for m in cands {
            let mut set = good.clone();
            set.extend(self.g.closure([m.clone()]));
            let out = self.test(format!("проверка: {}", self.g.name(m)), &set)?;
            if !out.ok() {
                // может, виновата его зависимость, а не он сам?
                let deps: Vec<String> = self.g.closure([m.clone()]).into_iter().filter(|d| d != m && !good.contains(d)).collect();
                if !deps.is_empty() {
                    let mut without = good.clone();
                    without.extend(self.g.closure(deps.clone()));
                    let o2 = self.test(format!("зависимости {}", self.g.name(m)), &without)?;
                    if !o2.ok() {
                        if let Some(found) = self.narrow(good, &deps)? {
                            return Some(Some(found));
                        }
                    }
                }
                return Some(Some((m.clone(), out)));
            }
        }
        Some(None)
    }
}

pub fn run<R: Runner>(g: &Graph, settings: &BisectSettings, runner: &mut R, on_step: &mut dyn FnMut(&Step)) -> Report {
    let mut cx = Ctx { g, r: runner, rep: Report::default(), on_step };

    // 0. без модов
    let Some(base) = cx.test("без модов".into(), &BTreeSet::new()) else { return finish(cx) };
    if !base.ok() {
        cx.rep.baseline_failed = true;
        cx.rep.message = format!("Сервер не запускается даже без модов: {}. Дело в ядре, конфиге или Java.", Ctx::<R>::reason(&base));
        return finish(cx);
    }

    let mut excluded: BTreeSet<String> = BTreeSet::new();
    loop {
        let all: Vec<String> = g.all.iter().filter(|m| !excluded.contains(*m)).cloned().collect();
        let found = if settings.mode == "binary" { binary(&mut cx, &all) } else { batch(&mut cx, &all, settings.batch.max(1)) };
        match found {
            None => return finish(cx), // отменено
            Some(None) => break,
            Some(Some((f, out))) => {
                cx.rep.culprits.push(Culprit { file: f.clone(), name: g.name(&f), reason: Ctx::<R>::reason(&out) });
                if !settings.find_all {
                    break;
                }
                excluded.insert(f.clone());
                excluded.extend(g.dependents(&f));
            }
        }
        if !cx.rep.group.is_empty() {
            break;
        }
    }
    if cx.rep.culprits.is_empty() && cx.rep.group.is_empty() && !cx.rep.cancelled {
        cx.rep.all_ok = true;
        cx.rep.message = "Со всеми модами тестовый сервер запустился — краш не повторился. Возможно, он зависит от мира или происходит во время игры.".into();
    }
    finish(cx)
}

fn finish<R: Runner>(cx: Ctx<'_, R>) -> Report {
    let mut r = cx.rep;
    if r.message.is_empty() {
        r.message = if r.cancelled {
            "Перебор остановлен".into()
        } else if !r.culprits.is_empty() {
            format!("Найдено: {}", r.culprits.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join(", "))
        } else if !r.group.is_empty() {
            "По одному моды запускаются, а вместе — нет: конфликт внутри группы".into()
        } else {
            String::new()
        };
    }
    r
}

/// Группы по N модов; Some(None) — виновник не найден.
fn batch<R: Runner>(cx: &mut Ctx<'_, R>, all: &[String], n: usize) -> Option<Option<(String, Outcome)>> {
    let mut good: BTreeSet<String> = BTreeSet::new();
    let mut i = 0;
    let mut group_no = 0;
    while i < all.len() {
        let chunk: Vec<String> = all[i..].iter().filter(|m| !good.contains(*m)).take(n).cloned().collect();
        if chunk.is_empty() {
            break;
        }
        // продвигаемся по исходному списку
        let last = chunk.last().cloned().unwrap();
        i = all.iter().position(|m| *m == last).map(|p| p + 1).unwrap_or(all.len());
        group_no += 1;
        let mut set = good.clone();
        set.extend(cx.g.closure(chunk.clone()));
        let out = cx.test(format!("группа {group_no}: +{}", chunk.len()), &set)?;
        if out.ok() {
            good = set;
            continue;
        }
        match cx.narrow(&good, &chunk)? {
            Some(found) => return Some(Some(found)),
            None => {
                cx.rep.group = chunk.iter().map(|f| cx.g.name(f)).collect();
                return Some(None);
            }
        }
    }
    Some(None)
}

/// Деление пополам.
fn binary<R: Runner>(cx: &mut Ctx<'_, R>, all: &[String]) -> Option<Option<(String, Outcome)>> {
    let full = cx.g.closure(all.to_vec());
    let out = cx.test(format!("все моды ({})", full.len()), &full)?;
    if out.ok() {
        return Some(None);
    }
    let mut good: BTreeSet<String> = BTreeSet::new();
    let mut cands: Vec<String> = all.to_vec();
    let mut last_fail = out;
    while cands.len() > 1 {
        let half: Vec<String> = cands[..cands.len() / 2].to_vec();
        let mut set = good.clone();
        set.extend(cx.g.closure(half.clone()));
        let o = cx.test(format!("половина: {} модов", half.len()), &set)?;
        if !o.ok() {
            cands = half;
            last_fail = o;
        } else {
            good = set;
            cands = cands[cands.len() / 2..].iter().filter(|m| !good.contains(*m)).cloned().collect();
            if cands.is_empty() {
                // всё «хорошее» по отдельности — конфликт между половинами
                cx.rep.group = all.iter().map(|f| cx.g.name(f)).take(10).collect();
                return Some(None);
            }
        }
    }
    let c = cands.into_iter().next()?;
    // подтверждение
    let mut set = good.clone();
    set.extend(cx.g.closure([c.clone()]));
    let o = cx.test(format!("проверка: {}", cx.g.name(&c)), &set)?;
    if o.ok() {
        let _ = last_fail;
        cx.rep.group = vec![cx.g.name(&c)];
        return Some(None);
    }
    Some(Some((c, o)))
}

// ===================================================================== тестовая копия

pub fn test_dir_for(server_dir: &Path) -> PathBuf {
    let name = server_dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "server".into());
    server_dir.with_file_name(format!("{name}.pcm-test"))
}

fn link_or_copy(src: &Path, dst: &Path, link: bool) -> Result<()> {
    if let Some(p) = dst.parent() {
        fs::create_dir_all(p)?;
    }
    if link && fs::hard_link(src, dst).is_ok() {
        return Ok(());
    }
    fs::copy(src, dst)?;
    Ok(())
}

/// Создаёт тестовую копию. Мир, логи, бэкапы и моды не копируются.
pub fn prepare_copy(server_dir: &Path, test_dir: &Path) -> Result<()> {
    if test_dir.exists() {
        fs::remove_dir_all(test_dir)?;
    }
    fs::create_dir_all(test_dir)?;
    let props = crate::props::read(server_dir);
    let level = props.iter().find(|(k, _)| k == "level-name").map(|(_, v)| v.clone()).unwrap_or_else(|| "world".into());
    let skip_top: Vec<String> = vec![
        level.clone(),
        format!("{level}_nether"),
        format!("{level}_the_end"),
        "logs".into(),
        "crash-reports".into(),
        "mods".into(),
        "mods_disabled".into(),
        "mods_old".into(),
        "plugins_old".into(),
        "cores_old".into(),
        "pcm-logs".into(),
        "backups".into(),
        "simplebackups".into(),
        ".cache".into(),
    ];
    let mut stack = vec![server_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir)?.flatten() {
            let p = e.path();
            let rel = p.strip_prefix(server_dir).unwrap_or(&p).to_path_buf();
            let top = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_string()).unwrap_or_default();
            if skip_top.contains(&top) || top.ends_with(".pcm-test") || top == "session.lock" {
                continue;
            }
            let ft = e.file_type()?;
            if ft.is_dir() {
                stack.push(p);
            } else if ft.is_file() {
                // jar и всё в libraries — ссылками (их никто не меняет), остальное — копиями
                let is_jar = p.extension().map(|x| x.eq_ignore_ascii_case("jar")).unwrap_or(false);
                let link = is_jar || top == "libraries";
                link_or_copy(&p, &test_dir.join(&rel), link)?;
            }
        }
    }
    fs::create_dir_all(test_dir.join("mods"))?;
    let port: u32 = props.iter().find(|(k, _)| k == "server-port").and_then(|(_, v)| v.parse().ok()).unwrap_or(25565);
    crate::props::write(
        test_dir,
        &[
            ("level-name".into(), "pcm-test-world".into()),
            ("server-port".into(), (port + 1000).min(65000).to_string()),
            ("enable-rcon".into(), "false".into()),
            ("enable-query".into(), "false".into()),
            ("motd".into(), "ProtoCore test".into()),
        ],
    )?;
    Ok(())
}

/// Ставит в mods/ тестовой копии ровно указанные файлы (ссылками из настоящей папки mods).
pub fn set_mods(server_dir: &Path, test_dir: &Path, files: &BTreeSet<String>) -> Result<()> {
    let dst = test_dir.join("mods");
    fs::create_dir_all(&dst)?;
    for e in fs::read_dir(&dst)?.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if e.path().is_file() && !files.contains(&n) {
            fs::remove_file(e.path())?;
        }
    }
    for f in files {
        let t = dst.join(f);
        if !t.exists() {
            let src = server_dir.join("mods").join(f);
            if !src.is_file() {
                return Err(Error::other(format!("нет файла {}", src.display())));
            }
            link_or_copy(&src, &t, true)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::{Dependency, ModInfo};

    fn m(file: &str, id: &str, deps: &[&str]) -> ModInfo {
        let mut meta = crate::mods::read_meta(Path::new("/nonexistent"));
        meta.ids = vec![id.into()];
        meta.name = id.to_uppercase();
        meta.deps = deps.iter().map(|d| Dependency { id: (*d).into(), required: true, version: "*".into(), side: Side::Both }).collect();
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
            wrong_deps: vec![],
            problems: vec![],
        }
    }

    fn report(n: usize) -> ModReport {
        let mut v: Vec<ModInfo> = (0..n).map(|i| m(&format!("m{i:02}.jar"), &format!("m{i:02}"), &[])).collect();
        v.push(m("lib.jar", "lib", &[]));
        v.push(m("user.jar", "user", &["lib"]));
        crate::mods::analyze(&mut v, None)
    }

    /// Фальшивый сервер: падает, если среди модов есть `bad` (или если есть оба из пары).
    struct Fake {
        bad: Vec<&'static str>,
        pair: Option<(&'static str, &'static str)>,
        runs: usize,
    }
    impl Runner for Fake {
        fn run(&mut self, _: &str, files: &BTreeSet<String>) -> Outcome {
            self.runs += 1;
            if self.bad.iter().any(|b| files.contains(*b)) {
                return Outcome::Crash("краш".into());
            }
            if let Some((a, b)) = self.pair {
                if files.contains(a) && files.contains(b) {
                    return Outcome::Crash("конфликт".into());
                }
            }
            Outcome::Ok
        }
    }

    #[test]
    fn batch_finds_single() {
        let r = report(23);
        let g = Graph::from_report(&r);
        let mut f = Fake { bad: vec!["m17.jar"], pair: None, runs: 0 };
        let rep = run(&g, &BisectSettings::default(), &mut f, &mut |_| {});
        assert_eq!(rep.culprits.len(), 1);
        assert_eq!(rep.culprits[0].file, "m17.jar");
        assert!(f.runs < 15, "{}", f.runs);
    }

    #[test]
    fn binary_finds_single() {
        let r = report(40);
        let g = Graph::from_report(&r);
        let mut f = Fake { bad: vec!["m33.jar"], pair: None, runs: 0 };
        let s = BisectSettings { mode: "binary".into(), ..Default::default() };
        let rep = run(&g, &s, &mut f, &mut |_| {});
        assert_eq!(rep.culprits[0].file, "m33.jar");
        assert!(f.runs <= 10, "{}", f.runs);
    }

    #[test]
    fn dependency_is_the_culprit() {
        let r = report(3);
        let g = Graph::from_report(&r);
        // user.jar сам по себе нормальный, падает его зависимость lib.jar
        let mut f = Fake { bad: vec!["lib.jar"], pair: None, runs: 0 };
        let rep = run(&g, &BisectSettings::default(), &mut f, &mut |_| {});
        assert_eq!(rep.culprits[0].file, "lib.jar");
    }

    #[test]
    fn pair_conflict_and_find_all_and_baseline() {
        let r = report(8);
        let g = Graph::from_report(&r);
        let mut f = Fake { bad: vec![], pair: Some(("m01.jar", "m02.jar")), runs: 0 };
        let rep = run(&g, &BisectSettings::default(), &mut f, &mut |_| {});
        assert!(rep.culprits.is_empty());
        assert_eq!(rep.group.len(), 5);

        let mut f2 = Fake { bad: vec!["m01.jar", "m06.jar"], pair: None, runs: 0 };
        let s = BisectSettings { find_all: true, ..Default::default() };
        let rep2 = run(&g, &s, &mut f2, &mut |_| {});
        let files: Vec<&str> = rep2.culprits.iter().map(|c| c.file.as_str()).collect();
        assert_eq!(files, vec!["m01.jar", "m06.jar"]);

        struct Broken;
        impl Runner for Broken {
            fn run(&mut self, _: &str, _: &BTreeSet<String>) -> Outcome {
                Outcome::Crash("нет Java".into())
            }
        }
        let rep3 = run(&g, &BisectSettings::default(), &mut Broken, &mut |_| {});
        assert!(rep3.baseline_failed);

        let mut ok = Fake { bad: vec![], pair: None, runs: 0 };
        assert!(run(&g, &BisectSettings::default(), &mut ok, &mut |_| {}).all_ok);
    }

    #[test]
    fn copy_skips_world_and_links_jars() {
        let t = tempfile::tempdir().unwrap();
        let s = t.path().join("srv");
        for d in ["world/region", "config", "mods", "libraries/a", "logs"] {
            fs::create_dir_all(s.join(d)).unwrap();
        }
        fs::write(s.join("world/region/r.0.0.mca"), "x").unwrap();
        fs::write(s.join("config/a.toml"), "cfg").unwrap();
        fs::write(s.join("mods/x.jar"), "jar").unwrap();
        fs::write(s.join("libraries/a/l.jar"), "lib").unwrap();
        fs::write(s.join("server.properties"), "server-port=25565\nlevel-name=world\n").unwrap();
        let td = test_dir_for(&s);
        prepare_copy(&s, &td).unwrap();
        assert!(!td.join("world").exists());
        assert!(td.join("config/a.toml").exists());
        assert!(td.join("libraries/a/l.jar").exists());
        assert!(fs::read_dir(td.join("mods")).unwrap().next().is_none());
        let props = crate::props::read(&td);
        assert!(props.contains(&("server-port".into(), "26565".into())));
        assert!(props.contains(&("level-name".into(), "pcm-test-world".into())));
        // конфиг — копия: правка в тесте не трогает настоящий
        fs::write(td.join("config/a.toml"), "changed").unwrap();
        assert_eq!(fs::read_to_string(s.join("config/a.toml")).unwrap(), "cfg");
        let set: BTreeSet<String> = ["x.jar".to_string()].into();
        set_mods(&s, &td, &set).unwrap();
        assert!(td.join("mods/x.jar").exists());
        set_mods(&s, &td, &BTreeSet::new()).unwrap();
        assert!(!td.join("mods/x.jar").exists());
        assert!(s.join("mods/x.jar").exists());
    }
}
