//! server.properties: чтение и аккуратная запись.
//! При записи сохраняются комментарии и порядок строк — меняются только значения.

use std::fs;
use std::path::Path;

use crate::Result;

/// Пары ключ=значение в порядке файла.
pub fn read(dir: &Path) -> Vec<(String, String)> {
    let Ok(s) = fs::read_to_string(dir.join("server.properties")) else { return vec![] };
    s.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Меняет указанные ключи, остальное оставляет как было. Новые ключи дописываются в конец.
pub fn write(dir: &Path, changes: &[(String, String)]) -> Result<()> {
    let path = dir.join("server.properties");
    let src = fs::read_to_string(&path).unwrap_or_default();
    let mut pending: Vec<(String, String)> = changes.to_vec();
    let mut out: Vec<String> = Vec::new();
    for line in src.lines() {
        if let Some((k, _)) = line.split_once('=') {
            if !line.trim_start().starts_with('#') {
                if let Some(pos) = pending.iter().position(|(pk, _)| pk == k.trim()) {
                    let (pk, pv) = pending.remove(pos);
                    out.push(format!("{pk}={pv}"));
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }
    for (k, v) in pending {
        out.push(format!("{k}={v}"));
    }
    let tmp = path.with_extension("properties.tmp");
    fs::write(&tmp, out.join("\n") + "\n")?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_comments() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("server.properties"), "#Minecraft\nmotd=Hi\nview-distance=12\n").unwrap();
        write(
            t.path(),
            &[("view-distance".into(), "10".into()), ("network-compression-threshold".into(), "256".into())],
        )
        .unwrap();
        let s = fs::read_to_string(t.path().join("server.properties")).unwrap();
        assert_eq!(s, "#Minecraft\nmotd=Hi\nview-distance=10\nnetwork-compression-threshold=256\n");
        assert_eq!(read(t.path()).len(), 3);
    }
}
