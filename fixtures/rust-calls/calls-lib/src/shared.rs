//! Mounted by the library as `shared` and, through `#[path]`, by its integration test.

pub fn shared_leaf() -> u32 {
    7
}

pub fn shared_call() -> u32 {
    shared_leaf()
}
