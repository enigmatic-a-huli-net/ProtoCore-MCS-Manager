fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let opt = pcm_core::modpack::ExportOptions {
        name: "ProtoCore SMP".into(), version: "1".into(), mc: "1.21.1".into(),
        core: pcm_core::detect::CoreKind::NeoForge, loader_version: "21.1.265".into(), include_config: false,
    };
    let r = pcm_core::modpack::export_client(&dir, &Default::default(), &opt, std::path::Path::new("/tmp/claude-0/client.mrpack"));
    println!("{r:?}");
    let i = pcm_core::modpack::inspect(std::path::Path::new("/tmp/claude-0/client.mrpack"));
    println!("{i:?}");
}
