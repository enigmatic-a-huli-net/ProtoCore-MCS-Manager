//! Записать все звуки оповещений в WAV: cargo run -p pcm-core --example sounds -- <папка>
use pcm_core::sound::{synth, Sound};

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&dir).unwrap();
    for s in [Sound::Start, Sound::Stop, Sound::Crash, Sound::Join, Sound::Warn] {
        std::fs::write(dir.join(format!("{}.wav", s.name())), synth(s, 80)).unwrap();
    }
}
