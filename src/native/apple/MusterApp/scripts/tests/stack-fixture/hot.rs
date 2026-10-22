use std::sync::atomic::{AtomicBool, Ordering};
static STOP: AtomicBool = AtomicBool::new(false);
#[unsafe(no_mangle)]
pub extern "C" fn fixture_stop() { STOP.store(true, Ordering::Relaxed); }
#[unsafe(no_mangle)]
pub extern "C" fn fixture_stopped() -> bool { STOP.load(Ordering::Relaxed) }
#[inline(never)]
fn rust_hot_inner() {
    let mut counter = 0u64;
    while !STOP.load(Ordering::Relaxed) {
        counter = std::hint::black_box(counter.wrapping_add(1));
    }
    std::hint::black_box(counter);
}
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn fixture_rust_hot() {
    rust_hot_inner();
    std::hint::black_box(42);
}
