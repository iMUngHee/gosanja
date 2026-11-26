mod a;
#[path = "a.rs"]
mod alias;
#[cfg_attr(feature = "x", path = "opt_x.rs")]
mod opt;
#[cfg(test)]
mod tests;
#[cfg(not(test))]
mod prod_only;
macro_rules! mount_in_body {
    () => {
        #[path = "fbody.rs"]
        mod fbody;
    };
}
pub fn holder() {
    mount_in_body!();
}
#[cfg_attr(not(test), derive(pm::Gen))]
pub struct S;
#[cfg(test)]
#[test]
fn unit() {}

#[cfg(test)]
pub fn gated() {}

#[inline(never)]
pub fn control_callee() -> usize {
    7
}

pub fn control_caller() -> usize {
    control_callee()
}

#[path = "shared.rs"]
mod shared;
