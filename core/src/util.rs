//! Мелкие вспомогательные функции.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;

use crate::Result;

/// Перемещает файл. Если rename не удался (другой диск), копирует и удаляет оригинал.
/// Если файл с таким именем уже есть в цели, добавляет суффикс " (2)", " (3)"...
pub fn move_file(src: &Path, dst_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dst_dir)?;
    let name = src
        .file_name()
        .ok_or_else(|| crate::Error::other("у файла нет имени"))?;
    let mut dst = dst_dir.join(name);
    if dst.exists() {
        let stem = src.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let ext = src
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let mut n = 2;
        while dst.exists() {
            dst = dst_dir.join(format!("{stem} ({n}){ext}"));
            n += 1;
        }
    }
    if fs::rename(src, &dst).is_err() {
        fs::copy(src, &dst)?;
        fs::remove_file(src)?;
    }
    Ok(dst)
}

/// Короткий случайный идентификатор (12 hex-символов).
pub fn short_id() -> String {
    use rand::Rng;
    let v: u64 = rand::thread_rng().gen();
    format!("{:012x}", v & 0xffff_ffff_ffff)
}

/// Случайный пароль для репозитория restic.
pub fn random_password() -> String {
    use rand::distributions::Alphanumeric;
    use rand::Rng;
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

/// Превращает картинку в data URI, уменьшая до `max` пикселей по большей стороне.
/// Маленькие иконки не раздувают память интерфейса даже при 200+ модах.
pub fn image_to_data_uri(bytes: &[u8], max: u32) -> Option<String> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > max || img.height() > max {
        img.resize(max, max, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(out.into_inner())
    ))
}

/// Любой файл как data URI (для GIF на экране загрузки и логотипа).
pub fn file_to_data_uri(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let mime = match path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .as_deref()
    {
        Some("gif") => "image/gif",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Имя файла как строка.
pub fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Делает строку безопасной для имени папки.
pub fn safe_dir_name(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "server".into()
    } else {
        trimmed.chars().take(40).collect()
    }
}

/// Все .jar в папке (без рекурсии), отсортированные по имени.
pub fn list_jars(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .map(|e| e.eq_ignore_ascii_case("jar"))
                    .unwrap_or(false)
        })
        .collect();
    v.sort_by_key(|p| file_name(p).to_lowercase());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_file_avoids_overwrite() {
        let t = tempfile::tempdir().unwrap();
        let a = t.path().join("a");
        let b = t.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("x.jar"), "1").unwrap();
        fs::write(b.join("x.jar"), "old").unwrap();
        let dst = move_file(&a.join("x.jar"), &b).unwrap();
        assert_eq!(file_name(&dst), "x (2).jar");
        assert_eq!(fs::read_to_string(b.join("x.jar")).unwrap(), "old");
    }

    #[test]
    fn safe_names() {
        assert_eq!(safe_dir_name("My Server: 1.21!"), "My_Server__1_21");
        assert_eq!(safe_dir_name("///"), "server");
    }
}
