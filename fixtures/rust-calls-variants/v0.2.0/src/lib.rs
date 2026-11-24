//! One small package, kept in three copies that differ only in their version or in the
//! features `default` enables.

pub fn leaf() -> u32 {
    1
}

pub fn direct() -> u32 {
    leaf()
}

#[cfg(feature = "extra")]
pub fn extra() -> u32 {
    leaf()
}
