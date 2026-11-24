//! Mounted by `#[path]` as `under_test` under `cfg(test)` and as `in_build` otherwise.

pub fn mounted() -> u32 {
    crate::leaf()
}
