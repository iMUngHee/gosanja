//! Begins with a byte order mark.

pub fn bom_leaf() -> u32 {
    4
}

pub fn bom_call() -> u32 {
    bom_leaf()
}
