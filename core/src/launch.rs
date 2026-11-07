//! Сборка команды запуска.
//!
//! Два режима:
//! - **Авто** — менеджер сам собирает `java <флаги> -jar сервер.jar nogui`
//!   (или `@libraries/.../win_args.txt` для Forge/NeoForge) из настроек.
//! - **Скрипт** — запускается твой .bat как есть. Удобно, если там что-то особенное.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{JvmPreset, LaunchMode, ServerEntry};
use crate::detect::{self, LaunchTarget};
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct LaunchCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// Готовая строка для показа в интерфейсе
    pub display: String,
}

/// Флаги Aikar — https://docs.papermc.io/paper/aikars-flags
pub fn aikar_flags(memory_mb: u32) -> Vec<String> {
    let big = memory_mb > 12 * 1024;
    let (new_min, new_max, region, reserve, ihop) =
        if big { (40, 50, "16M", 15, 20) } else { (30, 40, "8M", 20, 15) };
    [
        "-XX:+UseG1GC".to_string(),
        "-XX:+ParallelRefProcEnabled".into(),
        "-XX:MaxGCPauseMillis=200".into(),
        "-XX:+UnlockExperimentalVMOptions".into(),
        "-XX:+DisableExplicitGC".into(),
        "-XX:+AlwaysPreTouch".into(),
        format!("-XX:G1NewSizePercent={new_min}"),
        format!("-XX:G1MaxNewSizePercent={new_max}"),
        format!("-XX:G1HeapRegionSize={region}"),
        format!("-XX:G1ReservePercent={reserve}"),
        "-XX:G1HeapWastePercent=5".into(),
        "-XX:G1MixedGCCountTarget=4".into(),
        format!("-XX:InitiatingHeapOccupancyPercent={ihop}"),
        "-XX:G1MixedGCLiveThresholdPercent=90".into(),
        "-XX:G1RSetUpdatingPauseTimePercent=5".into(),
        "-XX:SurvivorRatio=32".into(),
        "-XX:+PerfDisableSharedMem".into(),
        "-XX:MaxTenuringThreshold=1".into(),
        "-Dusing.aikars.flags=https://mcflags.emc.gs".into(),
        "-Daikars.new.flags=true".into(),
    ]
    .to_vec()
}

/// Generational ZGC для Java 21+.
pub fn zgc_flags() -> Vec<String> {
    vec![
        "-XX:+UseZGC".into(),
        "-XX:+ZGenerational".into(),
        "-XX:+AlwaysPreTouch".into(),
        "-XX:+DisableExplicitGC".into(),
        "-XX:+PerfDisableSharedMem".into(),
    ]
}

/// Делит строку на аргументы с учётом кавычек: `-Da="x y" -Db` → [`-Da=x y`, `-Db`].
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                has = true;
            }
            None if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            None => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn quote_for_display(a: &str) -> String {
    if a.contains(' ') {
        format!("\"{a}\"")
    } else {
        a.to_string()
    }
}

pub fn build(server: &ServerEntry) -> Result<LaunchCommand> {
    let dir = server.dir();
    if !dir.is_dir() {
        return Err(Error::other(format!("папка сервера не найдена: {}", dir.display())));
    }
    let l = &server.launch;

    let (program, args) = match l.mode {
        LaunchMode::Script => {
            let script = resolve(&dir, &l.script);
            if !script.is_file() {
                return Err(Error::other(format!("скрипт не найден: {}", script.display())));
            }
            let s = script.to_string_lossy().to_string();
            if cfg!(windows) {
                ("cmd".to_string(), vec!["/C".to_string(), s])
            } else {
                ("sh".to_string(), vec![s])
            }
        }
        LaunchMode::Auto => {
            let java = if l.java_path.trim().is_empty() {
                // путь не указан — подбираем Java под версию Minecraft среди установленных
                let mc = if server.mc_version.is_empty() { detect::detect(&dir).mc_version } else { server.mc_version.clone() };
                crate::java::auto_java(&mc).unwrap_or_else(|| "java".to_string())
            } else {
                l.java_path.trim().to_string()
            };
            let mut args = Vec::new();

            // Forge/NeoForge хранят свои флаги в user_jvm_args.txt — подключаем первым,
            // наши флаги идут после и имеют приоритет (последний -Xmx выигрывает).
            let target = if l.jar_override.trim().is_empty() {
                detect::detect(&dir).target
            } else {
                LaunchTarget::Jar(l.jar_override.trim().to_string())
            };
            if matches!(target, LaunchTarget::ArgsFile(_)) && dir.join("user_jvm_args.txt").is_file() {
                args.push("@user_jvm_args.txt".to_string());
            }

            let mem = l.memory_mb.max(512);
            args.push(format!("-Xms{mem}M"));
            args.push(format!("-Xmx{mem}M"));
            match l.jvm_preset {
                JvmPreset::Aikar => args.extend(aikar_flags(mem)),
                JvmPreset::Zgc => args.extend(zgc_flags()),
                JvmPreset::None => {}
            }
            // Логи в UTF-8, чтобы кириллица в консоли не превращалась в «кракозябры»
            args.push("-Dfile.encoding=UTF-8".into());
            args.push("-Dstdout.encoding=UTF-8".into());
            args.push("-Dstderr.encoding=UTF-8".into());
            args.extend(split_args(&l.extra_jvm_args));

            match target {
                LaunchTarget::Jar(j) => {
                    args.push("-jar".into());
                    args.push(j);
                }
                LaunchTarget::ArgsFile(a) => args.push(format!("@{a}")),
                LaunchTarget::None => {
                    return Err(Error::other(
                        "не найден jar сервера. Укажи его в настройках запуска или выбери режим «Свой скрипт»",
                    ))
                }
            }
            args.extend(split_args(&l.server_args));
            (java, args)
        }
    };

    let display = std::iter::once(quote_for_display(&program))
        .chain(args.iter().map(|a| quote_for_display(a)))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(LaunchCommand { program, args, cwd: dir, display })
}

fn resolve(dir: &Path, p: &str) -> PathBuf {
    let p = PathBuf::from(p.trim());
    if p.is_absolute() {
        p
    } else {
        dir.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn split() {
        assert_eq!(split_args(r#"-Da="x y" -Db  nogui"#), vec!["-Da=x y", "-Db", "nogui"]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn build_auto_jar() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("youer-1.21.1-3068bd4.jar"), "").unwrap();
        let s = ServerEntry {
            path: t.path().to_string_lossy().into(),
            ..Default::default()
        };
        let c = build(&s).unwrap();
        assert!(c.program == "java" || c.program.contains("java"));
        assert!(c.args.contains(&"-Xmx8192M".to_string()));
        assert!(c.args.ends_with(&["-jar".into(), "youer-1.21.1-3068bd4.jar".into(), "nogui".into()]));
    }
}
