fn main() {
    for t in pcm_core::network::PROBE_TARGETS {
        println!("{t}: {:?}", pcm_core::network::probe(t, std::time::Duration::from_millis(1000)));
    }
    println!("local {:?}", pcm_core::network::local_ip());
}
