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
    /// ноты: (начало мс, частота Гц, длительность звучания мс, громкость 0–1)
    fn notes(self) -> &'static [(u32, f32, u32, f32)] {
        match self {
            // мягкое восходящее арпеджио — «готово»
            Sound::Start => &[(0, 523.25, 700, 0.8), (120, 659.25, 700, 0.75), (240, 783.99, 900, 0.7)],
            // нисходящая пара — «выключено»
            Sound::Stop => &[(0, 659.25, 600, 0.7), (150, 523.25, 800, 0.65)],
            // низкий минорный спуск — заметно, но не режет уши
            Sound::Crash => &[(0, 440.0, 700, 0.85), (220, 349.23, 700, 0.85), (440, 293.66, 1100, 0.9)],
            // короткий тихий «тинь»
            Sound::Join => &[(0, 880.0, 450, 0.45), (90, 1174.66, 550, 0.4)],
            // два одинаковых мягких сигнала
            Sound::Warn => &[(0, 587.33, 450, 0.7), (260, 587.33, 600, 0.7)],
        }
    }
}

const RATE: u32 = 44_100;

/// WAV (PCM 16 бит, моно). Тембр — мягкий «колокольчик»: синус с лёгкими обертонами,
/// плавной атакой и экспоненциальным затуханием; ноты звучат внахлёст.
pub fn synth(sound: Sound, volume: u32) -> Vec<u8> {
    let notes = sound.notes();
    let total_ms = notes.iter().map(|n| n.0 + n.2).max().unwrap_or(0) + 30;
    let len = (RATE as u64 * total_ms as u64 / 1000) as usize;
    let mut mix = vec![0f32; len];
    for &(start, freq, dur, gain) in notes {
        let s0 = (RATE as u64 * start as u64 / 1000) as usize;
        let n = (RATE as u64 * dur as u64 / 1000) as usize;
        let attack = RATE as f32 * 0.018;
        let tau = n as f32 / 4.5; // к концу ноты громкость падает почти до нуля
        for i in 0..n {
            if s0 + i >= len {
                break;
            }
            let t = i as f32 / RATE as f32;
            let env = (i as f32 / attack).min(1.0).powf(1.5) * (-(i as f32) / tau).exp();
            // обертоны быстро гаснут — начало чуть «звонче», хвост чистый
            let bright = (-(i as f32) / (tau * 0.35)).exp();
            let w = std::f32::consts::TAU * freq * t;
            let v = w.sin() + 0.18 * bright * (2.0 * w).sin() + 0.06 * bright * (3.0 * w).sin();
            mix[s0 + i] += v * env * gain;
        }
    }
    // мягкий фильтр низких частот (убирает «цифровую» резкость) и нормализация
    let mut prev = 0f32;
    for x in mix.iter_mut() {
        prev += 0.35 * (*x - prev);
        *x = prev;
    }
    let peak = mix.iter().fold(0f32, |m, x| m.max(x.abs())).max(1e-6);
    let amp = (volume.min(100) as f32 / 100.0) * 0.42 * i16::MAX as f32 / peak;
    let pcm: Vec<i16> = mix.iter().map(|x| (x * amp) as i16).collect();
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
        assert!(len > RATE as usize); // больше 0.5 с
        // нет щелчка в начале и конце
        let first = i16::from_le_bytes([w[44], w[45]]);
        let last = i16::from_le_bytes([w[w.len() - 2], w[w.len() - 1]]);
        assert!(first.abs() < 50 && last.abs() < 200, "{first} {last}");
        assert!(synth(Sound::Start, 0)[44..].iter().all(|b| *b == 0));
        let s = SoundSettings::default();
        assert!(Sound::Crash.enabled_in(&s) && !Sound::Join.enabled_in(&s));
    }
}
