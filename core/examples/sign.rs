//! Подпись релиза (запускается в CI).
//!   PCM_SIGNING_KEY=<hex 32 байта> cargo run -p pcm-core --example sign -- <файл>   → <файл>.sig
//!   PCM_SIGNING_KEY=<hex> cargo run -p pcm-core --example sign -- --pubkey          → публичный ключ (hex)
use ed25519_dalek::{Signer, SigningKey};

fn main() {
    let hex = std::env::var("PCM_SIGNING_KEY").unwrap_or_default();
    let hex = hex.trim();
    if hex.len() != 64 {
        eprintln!("PCM_SIGNING_KEY не задан или неверной длины");
        std::process::exit(2);
    }
    let mut seed = [0u8; 32];
    for i in 0..32 {
        seed[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("hex");
    }
    let key = SigningKey::from_bytes(&seed);
    let arg = std::env::args().nth(1).expect("укажи файл или --pubkey");
    if arg == "--pubkey" {
        println!("{}", to_hex(key.verifying_key().as_bytes()));
        return;
    }
    let data = std::fs::read(&arg).expect("файл не читается");
    let sig = key.sign(&data);
    std::fs::write(format!("{arg}.sig"), to_hex(&sig.to_bytes())).expect("запись .sig");
    println!("подписан {arg}");
}

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
