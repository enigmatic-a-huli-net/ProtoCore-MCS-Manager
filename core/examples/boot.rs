//! Запуск реального сервера командой менеджера, ожидание «Done» и выполнение команд.
//! cargo run -p pcm-core --example boot -- <папка> ["команда" ...]
use std::sync::Arc;
use std::time::Duration;
use pcm_core::process::*;
struct S;
impl EventSink for S {
    fn log(&self, _: &str, l: &[LogLine]) { for x in l { println!("{:?} {}", x.level, x.text) } }
    fn status(&self, _: &str, i: &StatusInfo) { println!("STATUS {:?}", i.status) }
    fn exited(&self, _: &str, i: &ExitInfo) { println!("EXIT {:?}", i) }
}
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut s = pcm_core::config::ServerEntry { id: "t".into(), path: a[1].clone(), ..Default::default() };
    s.launch.memory_mb = 2048;
    let cmd = pcm_core::launch::build(&s).unwrap();
    let pm = Arc::new(ProcessManager::new(Arc::new(S), 4000));
    pm.start("t", &cmd).unwrap();
    let done = pm.wait_for("t", r"Done \(", Duration::from_secs(240));
    println!("DONE? {}", done.is_some());
    for c in &a[2..] {
        println!("=== {c}");
        if let Some(q) = c.strip_prefix('?') {
            let lines = pm.query("t", q, pcm_core::tps::RESPONSE_PATTERN, Duration::from_millis(2500)).unwrap();
            println!("QUIET {} строк -> {:?}", lines.len(), pcm_core::tps::parse(&lines, q));
        } else {
            pm.send("t", c).unwrap();
            std::thread::sleep(Duration::from_secs(3));
        }
    }
    pm.stop_and_wait("t", Duration::from_secs(60)).unwrap();
    std::thread::sleep(Duration::from_secs(1));
}
