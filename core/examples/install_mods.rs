//! Поставить моды с Modrinth в сервер: cargo run --example install_mods -- <папка> <slug>...
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let s = pcm_core::config::ServerEntry {
        path: a[1].clone(),
        core: pcm_core::detect::CoreKind::NeoForge,
        mc_version: "1.21.1".into(),
        ..Default::default()
    };
    let t = pcm_core::catalog::Target::for_server(&s, pcm_core::catalog::Kind::Mod, "");
    for slug in &a[2..] {
        match pcm_core::catalog::plan_install(&t, pcm_core::catalog::Source::Modrinth, slug, None)
            .and_then(|p| pcm_core::catalog::apply(&t, &p, |_, _, _| {}))
        {
            Ok(f) => println!("{slug}: {f:?}"),
            Err(e) => println!("{slug}: ОШИБКА {e}"),
        }
    }
}
