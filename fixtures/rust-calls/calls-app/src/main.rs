unsafe extern "C" {
    safe fn shared_entry() -> u32;
    #[allow(non_snake_case)]
    safe fn _Read() -> u32;
}

fn helper() -> u32 {
    11
}

fn main() {
    let total = calls_lib::direct();
    let _ = (total, shared_entry());
    let _ = calls_lib::generic(7u64);
    let _ = calls_lib::hinted();
    let _ = _Read();
    let _ = helper();
}
