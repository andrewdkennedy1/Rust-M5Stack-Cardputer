#[no_mangle]
extern "Rust" fn __pender(_context: *mut ()) {}

fn main() {
    cardputer::runner::run();
}
