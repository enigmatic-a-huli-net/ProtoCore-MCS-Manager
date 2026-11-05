//! cargo run -p pcm-core --example install_core -- neoforge 1.21.1 /tmp/x
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let core: pcm_core::detect::CoreKind = serde_json::from_value(serde_json::json!(a[1])).unwrap();
    let dir = std::path::PathBuf::from(&a[3]);
    let r = pcm_core::cores::install(core, &a[2], "", &dir, "java", |l| println!("{l}"), |_, _| {});
    println!("{r:?}");
    println!("{:?}", pcm_core::detect::detect(&dir));
}
