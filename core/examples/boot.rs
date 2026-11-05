//! Запуск реального сервера командой менеджера и ожидание «Done».
use std::sync::Arc;
use std::time::Duration;
use pcm_core::process::*;
struct S;
impl EventSink for S {
    fn log(&self, _: &str, l: &[LogLine]) { for x in l { if x.level != Level::Info || x.text.contains("Done") || x.text.contains("Starting") { println!("{}", x.text) } } }
    fn status(&self, _: &str, i: &StatusInfo) { println!("STATUS {:?}", i.status) }
    fn exited(&self, _: &str, i: &ExitInfo) { println!("EXIT {:?}", i) }
}
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let mut s = pcm_core::config::ServerEntry { id: "t".into(), path: dir, ..Default::default() };
    s.launch.memory_mb = 2048;
    let cmd = pcm_core::launch::build(&s).unwrap();
    println!("CMD {}", cmd.display);
    let pm = Arc::new(ProcessManager::new(Arc::new(S), 2000));
    pm.start("t", &cmd).unwrap();
    let done = pm.wait_for("t", r"Done \(", Duration::from_secs(240));
    println!("DONE? {}", done.is_some());
    pm.stop_and_wait("t", Duration::from_secs(60)).unwrap();
    std::thread::sleep(Duration::from_secs(1));
}
