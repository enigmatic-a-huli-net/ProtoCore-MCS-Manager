//! Тестовая копия с одним набором модов и запуск: cargo run --example bisect_one -- <сервер> <мод.jar>...
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use pcm_core::process::*;
struct S;
impl EventSink for S {
    fn log(&self, _: &str, l: &[LogLine]) { for x in l { if x.level != Level::Info { println!("{:?} {}", x.level, x.text.chars().take(200).collect::<String>()) } } }
    fn status(&self, _: &str, i: &StatusInfo) { println!("STATUS {:?}", i.status) }
    fn exited(&self, _: &str, i: &ExitInfo) { println!("EXIT {:?}", i) }
}
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let real = std::path::PathBuf::from(&a[1]);
    let test = pcm_core::bisect::test_dir_for(&real);
    pcm_core::bisect::prepare_copy(&real, &test).unwrap();
    let set: BTreeSet<String> = a[2..].iter().cloned().collect();
    let pm = Arc::new(ProcessManager::new(Arc::new(S), 4000));
    for round in 0..3 {
        pcm_core::bisect::set_mods(&real, &test, &set).unwrap();
        let mut s = pcm_core::config::ServerEntry { id: "t".into(), path: test.to_string_lossy().into(), ..Default::default() };
        s.launch.memory_mb = 2048;
        let cmd = pcm_core::launch::build(&s).unwrap();
        pm.start("t", &cmd).unwrap();
        let t0 = std::time::Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(500));
            match pm.status("t").status {
                Status::Running => { println!("ROUND {round}: OK {:?}", t0.elapsed()); pm.stop_and_wait("t", Duration::from_secs(40)).unwrap(); break }
                Status::Crashed | Status::Stopped => { println!("ROUND {round}: FAIL {:?}", t0.elapsed()); break }
                _ => {}
            }
        }
    }
}
