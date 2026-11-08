//! Звуковые оповещения: запуск, остановка, падение, вход игрока, проблемы.
//! Звуки синтезируются на лету (никаких файлов), но можно подложить свои:
//! `branding/sounds/<событие>.wav` (start, stop, crash, join, warn).
//! Играются из ядра, поэтому работают и когда окно свёрнуто в трей.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundSettings {
    pub enabled: bool,
    /// 0–100
    pub volume: u32,
    pub on_start: bool,
    pub on_stop: bool,
    pub on_crash: bool,
    pub on_join: bool,
    /// сбой бэкапа, низкий TPS, мало места
    pub on_warn: bool,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self { enabled: true, volume: 60, on_start: true, on_stop: false, on_crash: true, on_join: false, on_warn: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sound {
    Start,
    Stop,
    Crash,
    Join,
    Warn,
}

impl Sound {
    pub fn name(self) -> &'static str {
        match self {
            Sound::Start => "start",
            Sound::Stop => "stop",
            Sound::Crash => "crash",
            Sound::Join => "join",
            Sound::Warn => "warn",
        }
    }
    pub fn enabled_in(self, s: &SoundSettings) -> bool {
        s.enabled
            && s.volume > 0
            && match self {
                Sound::Start => s.on_start,
                Sound::Stop => s.on_stop,
                Sound::Crash => s.on_crash,
                Sound::Join => s.on_join,
                Sound::Warn => s.on_warn,
            }
    }
    /// ноты: (частота Гц, длительность мс, «жёсткость» 0 — синус, 1 — квадрат)
    fn notes(self) -> &'static [(f32, u32, f32)] {
        match self {
            // восходящее трезвучие — «готово»
            Sound::Start => &[(523.25, 110, 0.0), (659.25, 110, 0.0), (783.99, 220, 0.0)],
            // нисходящее — «выключено»
            Sound::Stop => &[(783.99, 120, 0.0), (523.25, 220, 0.0)],
            // три низких тревожных сигнала
            Sound::Crash => &[(220.0, 170, 0.35), (0.0, 60, 0.0), (220.0, 170, 0.35), (0.0, 60, 0.0), (174.61, 320, 0.35)],
            // короткий мягкий «дзынь»
            Sound::Join => &[(1318.5, 70, 0.0), (1760.0, 120, 0.0)],
            // два средних сигнала
            Sound::Warn => &[(659.25, 140, 0.15), (0.0, 70, 0.0), (659.25, 140, 0.15)],
        }
    }
}

const RATE: u32 = 22_050;

/// WAV (PCM 16 бит, моно) с нотами звука.
pub fn synth(sound: Sound, volume: u32) -> Vec<u8> {
    let amp = (volume.min(100) as f32 / 100.0) * 0.55 * i16::MAX as f32;
    let mut pcm: Vec<i16> = Vec::new();
    for &(freq, ms, hard) in sound.notes() {
        let n = (RATE * ms / 1000) as usize;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            // огибающая: быстрый вход, плавный хвост — без щелчков
            let attack = (i as f32 / (RATE as f32 * 0.008)).min(1.0);
            let release = ((n - i) as f32 / (n as f32 * 0.6)).min(1.0);
            let env = attack * release * release;
            let s = if freq <= 0.0 {
                0.0
            } else {
                let ph = (t * freq * std::f32::consts::TAU).sin();
                let sq = ph.signum();
                (ph * (1.0 - hard) + sq * hard * 0.6) * env
            };
            pcm.push((s * amp) as i16);
        }
    }
    let data_len = (pcm.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // моно
    w.extend_from_slice(&RATE.to_le_bytes());
    w.extend_from_slice(&(RATE * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

/// Сыграть в фоне. `branding` — папка со своими звуками (может не быть).
pub fn play(sound: Sound, settings: &SoundSettings, branding: &Path, force: bool) {
    if !force && !sound.enabled_in(settings) {
        return;
    }
    let custom = branding.join("sounds").join(format!("{}.wav", sound.name()));
    let custom = custom.is_file().then_some(custom);
    let volume = settings.volume;
    std::thread::spawn(move || imp::play(custom.as_deref(), sound, volume));
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_FILENAME, SND_MEMORY, SND_NODEFAULT, SND_SYNC};

    pub fn play(custom: Option<&Path>, sound: Sound, volume: u32) {
        unsafe {
            if let Some(p) = custom {
                let w: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
                PlaySoundW(w.as_ptr(), std::ptr::null_mut(), SND_FILENAME | SND_NODEFAULT | SND_SYNC);
            } else {
                let wav = synth(sound, volume);
                PlaySoundW(wav.as_ptr() as *const u16, std::ptr::null_mut(), SND_MEMORY | SND_NODEFAULT | SND_SYNC);
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub fn play(_custom: Option<&Path>, _sound: Sound, _volume: u32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_is_valid() {
        let w = synth(Sound::Crash, 60);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        let len = u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize;
        assert_eq!(w.len(), 44 + len);
        assert!(len > RATE as usize); // ~0.8 c
        assert!(synth(Sound::Start, 0)[44..].iter().all(|b| *b == 0));
        let s = SoundSettings::default();
        assert!(Sound::Crash.enabled_in(&s) && !Sound::Join.enabled_in(&s));
    }
}
