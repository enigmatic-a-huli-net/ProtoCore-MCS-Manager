//! Процесс сервера: запуск, остановка, консоль, статус, ОЗУ/CPU.
//!
//! Устройство:
//! - на каждый запущенный сервер есть [`Instance`] с кольцевым буфером логов;
//! - два потока читают stdout/stderr и шлют строки в поток-«сборщик»;
//! - сборщик разбирает строки (статус, игроки), будит ожидающих и раз в 100 мс
//!   пачкой отправляет новые строки в интерфейс через [`EventSink`];
//! - поток-наблюдатель ждёт завершения процесса и сообщает, было ли это падение.
//!
//! Если окно закрыто (свёрнуто в трей), логи продолжают копиться в буфере,
//! а интерфейс при открытии просто забирает буфер целиком.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use regex::Regex;
use serde::Serialize;

use crate::launch::LaunchCommand;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
    /// Сообщения самого менеджера
    System,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub seq: u64,
    /// Время, мс с начала эпохи
    pub ts: i64,
    pub level: Level,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusInfo {
    pub status: Status,
    pub pid: Option<u32>,
    /// Секунд с момента запуска
    pub uptime: u64,
    pub players: Vec<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExitInfo {
    pub code: Option<i32>,
    /// true — сервер завершился сам, а не по команде остановки
    pub crashed: bool,
    /// Сколько секунд проработал
    pub uptime: u64,
    /// Успел ли сервер полностью запуститься («Done»)
    pub reached_running: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Metrics {
    /// Загрузка CPU, % от одного ядра (как в диспетчере задач × число ядер)
    pub cpu: f32,
    pub ram_bytes: u64,
    pub system_ram_total: u64,
    pub cores: usize,
}

/// Куда отправлять события (реализуется в src-tauri).
pub trait EventSink: Send + Sync {
    fn log(&self, server_id: &str, lines: &[LogLine]);
    fn status(&self, server_id: &str, info: &StatusInfo);
    fn exited(&self, server_id: &str, info: &ExitInfo);
}

struct Waiter {
    re: Regex,
    tx: Sender<String>,
}

struct Inner {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    status: Status,
    pid: Option<u32>,
    started: Option<Instant>,
    players: BTreeSet<String>,
    stop_requested: bool,
    exit_code: Option<i32>,
    reached_running: bool,
}

/// Перехват ответа на служебную команду (TPS): строки не попадают в консоль.
struct Quiet {
    re: Regex,
    until: Instant,
    tx: Sender<String>,
}

pub struct Instance {
    quiet: Mutex<Option<Quiet>>,
    id: String,
    inner: Mutex<Inner>,
    log: Mutex<VecDeque<LogLine>>,
    cap: usize,
    waiters: Mutex<Vec<Waiter>>,
}

static SEQ: AtomicU64 = AtomicU64::new(1);

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn re_ansi() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").unwrap())
}
fn re_done() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"Done \(\d+[.,]?\d*s\)! For help").unwrap())
}
fn re_join() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\]: (\.?[A-Za-z0-9_]{2,16}) joined the game").unwrap())
}
fn re_leave() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\]: (\.?[A-Za-z0-9_]{2,16}) left the game").unwrap())
}

/// Определяет уровень строки лога по типичным форматам log4j.
pub fn detect_level(s: &str) -> Level {
    let head: String = s.chars().take(160).collect();
    if head.contains("/ERROR]") || head.contains("[ERROR]") || head.contains("/FATAL]") || head.contains(" ERROR ") {
        Level::Error
    } else if head.contains("/WARN]") || head.contains("[WARN]") || head.contains(" WARN ") {
        Level::Warn
    } else if s.starts_with("\tat ") || s.starts_with("Caused by:") || s.starts_with("Exception in thread") || is_exception_head(s) {
        Level::Error
    } else {
        Level::Info
    }
}

/// «java.lang.RuntimeException: …» — первая строка исключения.
fn is_exception_head(s: &str) -> bool {
    let head = s.split(':').next().unwrap_or("");
    head.contains('.')
        && !head.contains(' ')
        && (head.ends_with("Exception") || head.ends_with("Error"))
        && head.chars().next().is_some_and(|c| c.is_ascii_lowercase())
}

impl Instance {
    fn new(id: &str, cap: usize) -> Self {
        Self {
            quiet: Mutex::new(None),
            id: id.to_string(),
            inner: Mutex::new(Inner {
                child: None,
                stdin: None,
                status: Status::Stopped,
                pid: None,
                started: None,
                players: BTreeSet::new(),
                stop_requested: false,
                exit_code: None,
                reached_running: false,
            }),
            log: Mutex::new(VecDeque::with_capacity(cap.min(10_000))),
            cap: cap.max(200),
            waiters: Mutex::new(Vec::new()),
        }
    }

    fn status_info(&self) -> StatusInfo {
        let i = self.inner.lock().unwrap();
        StatusInfo {
            status: i.status,
            pid: i.pid,
            uptime: i.started.map(|s| s.elapsed().as_secs()).unwrap_or(0),
            players: i.players.iter().cloned().collect(),
            exit_code: i.exit_code,
        }
    }

    fn push(&self, level: Level, text: String) -> LogLine {
        let line = LogLine { seq: SEQ.fetch_add(1, Ordering::Relaxed), ts: now_ms(), level, text };
        let mut log = self.log.lock().unwrap();
        if log.len() >= self.cap {
            log.pop_front();
        }
        log.push_back(line.clone());
        line
    }
}

pub struct ProcessManager {
    sink: Arc<dyn EventSink>,
    instances: Mutex<HashMap<String, Arc<Instance>>>,
    sys: Mutex<sysinfo::System>,
    console_cap: Mutex<usize>,
}

impl ProcessManager {
    pub fn new(sink: Arc<dyn EventSink>, console_lines: usize) -> Self {
        Self {
            sink,
            instances: Mutex::new(HashMap::new()),
            sys: Mutex::new(sysinfo::System::new()),
            console_cap: Mutex::new(console_lines),
        }
    }

    fn instance(&self, id: &str) -> Arc<Instance> {
        let cap = *self.console_cap.lock().unwrap();
        self.instances
            .lock()
            .unwrap()
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(Instance::new(id, cap)))
            .clone()
    }

    pub fn status(&self, id: &str) -> StatusInfo {
        self.instance(id).status_info()
    }

    pub fn is_running(&self, id: &str) -> bool {
        matches!(
            self.status(id).status,
            Status::Starting | Status::Running | Status::Stopping
        )
    }

    /// Все строки консоли из буфера (или только новее `after_seq`).
    pub fn log_buffer(&self, id: &str, after_seq: u64) -> Vec<LogLine> {
        let inst = self.instance(id);
        let log = inst.log.lock().unwrap();
        log.iter().filter(|l| l.seq > after_seq).cloned().collect()
    }

    pub fn clear_log(&self, id: &str) {
        self.instance(id).log.lock().unwrap().clear();
    }

    /// Сообщение менеджера в консоль сервера (выделяется цветом).
    pub fn system(&self, id: &str, text: impl Into<String>) {
        let inst = self.instance(id);
        let line = inst.push(Level::System, text.into());
        self.sink.log(id, &[line]);
    }

    fn set_status(&self, inst: &Instance, st: Status) {
        inst.inner.lock().unwrap().status = st;
        self.sink.status(&inst.id, &inst.status_info());
    }

    pub fn start(self: &Arc<Self>, id: &str, cmd: &LaunchCommand) -> Result<()> {
        let inst = self.instance(id);
        {
            let i = inst.inner.lock().unwrap();
            if matches!(i.status, Status::Starting | Status::Running | Status::Stopping) {
                return Err(Error::AlreadyRunning);
            }
        }

        self.system(id, format!("▶ Запуск: {}", cmd.display));

        let mut c = Command::new(&cmd.program);
        c.args(&cmd.args)
            .current_dir(&cmd.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            c.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = c.spawn().map_err(|e| {
            let hint = if e.kind() == std::io::ErrorKind::NotFound {
                " — проверь путь к Java в настройках запуска"
            } else {
                ""
            };
            Error::other(format!("не удалось запустить «{}»: {e}{hint}", cmd.program))
        })?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdin = child.stdin.take();
        let pid = child.id();
        {
            let mut i = inst.inner.lock().unwrap();
            i.child = Some(child);
            i.stdin = stdin;
            i.pid = Some(pid);
            i.started = Some(Instant::now());
            i.players.clear();
            i.stop_requested = false;
            i.exit_code = None;
            i.reached_running = false;
            i.status = Status::Starting;
        }
        self.sink.status(id, &inst.status_info());

        let (tx, rx) = mpsc::channel::<(bool, String)>();
        if let Some(out) = stdout {
            spawn_reader(out, false, tx.clone());
        }
        if let Some(err) = stderr {
            spawn_reader(err, true, tx.clone());
        }
        drop(tx);

        // сборщик; по окончании сообщает наблюдателю, что все строки уже в буфере
        let (done_tx, done_rx) = mpsc::channel::<()>();
        {
            let mgr = Arc::clone(self);
            let inst = Arc::clone(&inst);
            thread::Builder::new()
                .name(format!("pcm-log-{id}"))
                .spawn(move || {
                    mgr.collector(inst, rx);
                    let _ = done_tx.send(());
                })
                .ok();
        }
        // наблюдатель
        {
            let mgr = Arc::clone(self);
            let inst = Arc::clone(&inst);
            thread::Builder::new()
                .name(format!("pcm-wait-{id}"))
                .spawn(move || mgr.watcher(inst, done_rx))
                .ok();
        }
        Ok(())
    }

    fn collector(&self, inst: Arc<Instance>, rx: Receiver<(bool, String)>) {
        let mut batch: Vec<LogLine> = Vec::new();
        let mut last_flush = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok((is_err, raw)) => {
                    let text = re_ansi().replace_all(raw.trim_end_matches(['\r', '\n']), "").to_string();
                    let mut level = detect_level(&text);
                    if is_err && level == Level::Info {
                        level = Level::Warn;
                    }
                    // ответ на тихий запрос — не показываем в консоли
                    {
                        let mut q = inst.quiet.lock().unwrap();
                        if let Some(qq) = q.as_ref() {
                            if Instant::now() > qq.until {
                                *q = None;
                            } else if qq.re.is_match(&text) {
                                let _ = qq.tx.send(text);
                                continue;
                            }
                        }
                    }
                    self.analyze_line(&inst, &text);
                    batch.push(inst.push(level, text));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if !batch.is_empty() {
                        self.sink.log(&inst.id, &batch);
                    }
                    break;
                }
            }
            if !batch.is_empty() && (last_flush.elapsed() >= Duration::from_millis(100) || batch.len() > 500) {
                self.sink.log(&inst.id, &batch);
                batch.clear();
                last_flush = Instant::now();
            }
        }
    }

    fn analyze_line(&self, inst: &Instance, text: &str) {
        // ожидающие (например, «Saved the game» при бэкапе)
        {
            let mut w = inst.waiters.lock().unwrap();
            w.retain(|wt| {
                if wt.re.is_match(text) {
                    let _ = wt.tx.send(text.to_string());
                    false
                } else {
                    true
                }
            });
        }
        let mut changed = false;
        {
            let mut i = inst.inner.lock().unwrap();
            if i.status == Status::Starting && re_done().is_match(text) {
                i.status = Status::Running;
                i.reached_running = true;
                changed = true;
            }
            if let Some(c) = re_join().captures(text) {
                changed |= i.players.insert(c[1].to_string());
            }
            if let Some(c) = re_leave().captures(text) {
                changed |= i.players.remove(&c[1]);
            }
        }
        if changed {
            self.sink.status(&inst.id, &inst.status_info());
        }
    }

    fn watcher(&self, inst: Arc<Instance>, collector_done: Receiver<()>) {
        let code = loop {
            {
                let mut i = inst.inner.lock().unwrap();
                match i.child.as_mut().map(|c| c.try_wait()) {
                    Some(Ok(Some(st))) => break st.code(),
                    Some(Ok(None)) => {}
                    Some(Err(_)) | None => break None,
                }
            }
            thread::sleep(Duration::from_millis(400));
        };
        // ждём, пока последние строки лога (обычно самые важные — ошибка) попадут в буфер
        let _ = collector_done.recv_timeout(Duration::from_secs(5));
        let (crashed, uptime, reached_running) = {
            let mut i = inst.inner.lock().unwrap();
            let crashed = !i.stop_requested;
            i.child = None;
            i.stdin = None;
            i.pid = None;
            i.exit_code = code;
            i.players.clear();
            i.status = if crashed { Status::Crashed } else { Status::Stopped };
            let up = i.started.take().map(|s| s.elapsed().as_secs()).unwrap_or(0);
            (crashed, up, i.reached_running)
        };
        let msg = match (crashed, code) {
            (false, _) => "■ Сервер остановлен".to_string(),
            (true, Some(c)) => format!("✖ Сервер завершился сам (код {c}) — похоже на падение"),
            (true, None) => "✖ Сервер завершился сам — похоже на падение".to_string(),
        };
        self.system(&inst.id, msg);
        self.sink.status(&inst.id, &inst.status_info());
        self.sink.exited(&inst.id, &ExitInfo { code, crashed, uptime, reached_running });
    }

    /// Отправить команду в консоль сервера.
    pub fn send(&self, id: &str, command: &str) -> Result<()> {
        let inst = self.instance(id);
        let mut i = inst.inner.lock().unwrap();
        let stdin = i.stdin.as_mut().ok_or(Error::NotRunning)?;
        let cmd = command.trim().trim_start_matches('/');
        stdin.write_all(format!("{cmd}\n").as_bytes())?;
        stdin.flush()?;
        Ok(())
    }

    /// Тихая команда: отправить и собрать строки ответа, подходящие под `pattern`,
    /// в течение `window`. Эти строки не попадают в консоль.
    pub fn query(&self, id: &str, command: &str, pattern: &str, window: Duration) -> Result<Vec<String>> {
        let inst = self.instance(id);
        let re = Regex::new(pattern).map_err(|e| Error::other(e.to_string()))?;
        let (tx, rx) = mpsc::channel();
        *inst.quiet.lock().unwrap() = Some(Quiet { re, until: Instant::now() + window, tx });
        {
            let mut i = inst.inner.lock().unwrap();
            let stdin = i.stdin.as_mut().ok_or(Error::NotRunning)?;
            stdin.write_all(format!("{}\n", command.trim()).as_bytes())?;
            stdin.flush()?;
        }
        let deadline = Instant::now() + window;
        let mut out = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(l) => out.push(l),
                Err(_) => break,
            }
        }
        *inst.quiet.lock().unwrap() = None;
        Ok(out)
    }

    /// Мягкая остановка: `stop`, затем через `timeout` — принудительно.
    pub fn stop(self: &Arc<Self>, id: &str, timeout: Duration) -> Result<()> {
        let inst = self.instance(id);
        let pid = {
            let mut i = inst.inner.lock().unwrap();
            if i.child.is_none() {
                return Err(Error::NotRunning);
            }
            i.stop_requested = true;
            i.pid
        };
        self.set_status(&inst, Status::Stopping);
        self.system(id, "■ Остановка (команда stop)…");
        if self.send(id, "stop").is_err() {
            return self.kill(id);
        }
        let mgr = Arc::clone(self);
        let id = id.to_string();
        // «сторож» относится только к ЭТОМУ процессу: если сервер уже перезапущен
        // (другой pid), он ничего не трогает
        thread::spawn(move || {
            let same = |m: &ProcessManager| m.is_running(&id) && m.status(&id).pid == pid;
            let t0 = Instant::now();
            while t0.elapsed() < timeout {
                if !same(&mgr) {
                    return;
                }
                thread::sleep(Duration::from_millis(300));
            }
            if same(&mgr) {
                mgr.system(&id, "⚠ Сервер не остановился вовремя — завершаю принудительно");
                let _ = mgr.kill_pid(&id, pid);
            }
        });
        Ok(())
    }

    /// Блокирующая остановка: ждёт, пока процесс реально завершится.
    pub fn stop_and_wait(self: &Arc<Self>, id: &str, timeout: Duration) -> Result<()> {
        if !self.is_running(id) {
            return Ok(());
        }
        self.stop(id, timeout)?;
        let t0 = Instant::now();
        while self.is_running(id) {
            if t0.elapsed() > timeout + Duration::from_secs(15) {
                return Err(Error::other("сервер не остановился"));
            }
            thread::sleep(Duration::from_millis(250));
        }
        Ok(())
    }

    /// Завершить, только если это всё ещё тот же процесс.
    fn kill_pid(&self, id: &str, pid: Option<u32>) -> Result<()> {
        if self.status(id).pid != pid {
            return Ok(());
        }
        self.kill(id)
    }

    /// Принудительное завершение (вместе с дочерними процессами).
    pub fn kill(&self, id: &str) -> Result<()> {
        let inst = self.instance(id);
        let mut i = inst.inner.lock().unwrap();
        i.stop_requested = true;
        let pid = i.pid.ok_or(Error::NotRunning)?;
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // /T — всё дерево: при запуске через .bat java — дочерний процесс cmd
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x0800_0000)
                .status();
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            if let Some(c) = i.child.as_mut() {
                let _ = c.kill();
            }
        }
        Ok(())
    }

    /// Ждать строку лога, подходящую под регулярку (например, «Saved the game»).
    pub fn wait_for(&self, id: &str, pattern: &str, timeout: Duration) -> Option<String> {
        let inst = self.instance(id);
        let re = Regex::new(pattern).ok()?;
        let (tx, rx) = mpsc::channel();
        inst.waiters.lock().unwrap().push(Waiter { re, tx });
        rx.recv_timeout(timeout).ok()
    }

    /// ОЗУ и CPU процесса сервера (с дочерними — важно при запуске через .bat).
    pub fn metrics(&self, id: &str) -> Metrics {
        let pid = self.status(id).pid;
        let mut sys = self.sys.lock().unwrap();
        sys.refresh_memory();
        let mut m = Metrics {
            system_ram_total: sys.total_memory(),
            cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
            ..Default::default()
        };
        let Some(pid) = pid else { return m };
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let root = sysinfo::Pid::from_u32(pid);
        // собираем дерево процессов от корня
        let mut tree = vec![root];
        let mut idx = 0;
        while idx < tree.len() {
            let parent = tree[idx];
            for (p, proc_) in sys.processes() {
                // на Linux потоки видны как «процессы» с той же памятью — их не считаем
                if proc_.thread_kind().is_some() {
                    continue;
                }
                if proc_.parent() == Some(parent) && !tree.contains(p) {
                    tree.push(*p);
                }
            }
            idx += 1;
        }
        for p in tree {
            if let Some(pr) = sys.process(p) {
                m.cpu += pr.cpu_usage();
                m.ram_bytes += pr.memory();
            }
        }
        m
    }

    pub fn running_ids(&self) -> Vec<String> {
        self.instances
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, i)| i.inner.lock().unwrap().child.is_some())
            .map(|(k, _)| k.clone())
            .collect()
    }
}

fn spawn_reader<R: std::io::Read + Send + 'static>(r: R, is_err: bool, tx: Sender<(bool, String)>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(r);
        let mut buf = Vec::with_capacity(512);
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    // логи бывают не в UTF-8 (старые моды, cp1251) — не падаем, а заменяем символы
                    let s = String::from_utf8_lossy(&buf).to_string();
                    if tx.send((is_err, s)).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels() {
        assert_eq!(detect_level("[12:00:00] [Server thread/WARN] [x]: hi"), Level::Warn);
        assert_eq!(detect_level("[12:00:00] [main/ERROR] [x]: bad"), Level::Error);
        assert_eq!(detect_level("\tat net.minecraft.Foo.bar(Foo.java:1)"), Level::Error);
        assert_eq!(detect_level("[12:00:00] [Server thread/INFO]: ok"), Level::Info);
    }

    struct Sink(Mutex<Vec<String>>, Mutex<Vec<ExitInfo>>);
    impl EventSink for Sink {
        fn log(&self, _: &str, lines: &[LogLine]) {
            self.0.lock().unwrap().extend(lines.iter().map(|l| l.text.clone()));
        }
        fn status(&self, _: &str, _: &StatusInfo) {}
        fn exited(&self, _: &str, info: &ExitInfo) {
            self.1.lock().unwrap().push(info.clone());
        }
    }

    /// Регрессия: «сторож» остановки не должен убивать сервер, запущенный сразу после остановки.
    #[cfg(unix)]
    #[test]
    fn stale_stop_watchdog_does_not_kill_next_run() {
        let t = tempfile::tempdir().unwrap();
        let script = t.path().join("s.sh");
        std::fs::write(
            &script,
            "echo '[00:00:00] [Server thread/INFO]: Done (0.1s)! For help, type \"help\"'\n\
             while read l; do if [ \"$l\" = stop ]; then exit 0; fi; done\n",
        )
        .unwrap();
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(vec![])));
        let mgr = Arc::new(ProcessManager::new(sink.clone(), 1000));
        let cmd = LaunchCommand { program: "sh".into(), args: vec![script.to_string_lossy().into()], cwd: t.path().into(), display: "s".into() };
        mgr.start("x", &cmd).unwrap();
        thread::sleep(Duration::from_millis(400));
        // короткий таймаут сторожа: 2 с
        mgr.stop_and_wait("x", Duration::from_secs(2)).unwrap();
        mgr.start("x", &cmd).unwrap();
        thread::sleep(Duration::from_secs(3)); // сторож прошлого запуска уже «истёк»
        assert_eq!(mgr.status("x").status, Status::Running, "новый процесс убит старым сторожем");
        mgr.stop_and_wait("x", Duration::from_secs(5)).unwrap();
    }

    /// Имитация сервера: sh-скрипт печатает «Done», ждёт stop и выходит.
    #[cfg(unix)]
    #[test]
    fn start_detect_done_and_stop() {
        let t = tempfile::tempdir().unwrap();
        let script = t.path().join("fake.sh");
        std::fs::write(
            &script,
            "echo '[00:00:00] [Server thread/INFO]: Done (1.23s)! For help, type \"help\"'\n\
             echo '[00:00:01] [Server thread/INFO]: Steve joined the game'\n\
             while read l; do echo \"got $l\"; if [ \"$l\" = stop ]; then exit 0; fi; done\n",
        )
        .unwrap();
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(vec![])));
        let mgr = Arc::new(ProcessManager::new(sink.clone(), 1000));
        let cmd = LaunchCommand {
            program: "sh".into(),
            args: vec![script.to_string_lossy().into()],
            cwd: t.path().into(),
            display: "sh fake.sh".into(),
        };
        mgr.start("s1", &cmd).unwrap();
        assert!(mgr.wait_for("s1", "joined", Duration::from_secs(5)).is_some() || mgr.status("s1").players.len() == 1);
        thread::sleep(Duration::from_millis(300));
        let st = mgr.status("s1");
        assert_eq!(st.status, Status::Running);
        assert_eq!(st.players, vec!["Steve".to_string()]);
        mgr.stop_and_wait("s1", Duration::from_secs(5)).unwrap();
        thread::sleep(Duration::from_millis(300));
        assert_eq!(mgr.status("s1").status, Status::Stopped);
        assert!(!sink.1.lock().unwrap()[0].crashed);
        assert!(sink.0.lock().unwrap().iter().any(|l| l == "got stop"));
    }
}
