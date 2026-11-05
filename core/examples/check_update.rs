//! Проверка обновлений из консоли: cargo run -p pcm-core --example check_update
fn main() {
    match pcm_core::update::check(pcm_core::update::DEFAULT_REPO, "0.0.1") {
        Ok(u) => println!("последняя: {:?}, доступно: {}", u.latest, u.available),
        Err(e) => println!("ошибка: {e}"),
    }
}
