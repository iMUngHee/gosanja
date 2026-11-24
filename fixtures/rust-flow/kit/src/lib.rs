#![allow(dead_code)]

macro_rules! slot { () => { fn generated() {} }; }
slot! {}

pub struct Op { pub f: fn(i32) -> i32 }
pub enum Flag { On, Off }
pub union Bits { pub word: u32, pub byte: u8 }
pub type Count = i32;
pub const LIMIT: Count = 4;
pub static mut TOTAL: Count = 0;

pub trait Adjust {
    type Output;
    const START: i32;
    fn required(&self);
    slot! {}
}

impl Op {
    pub fn apply(&self, x: i32) -> i32 { (self.f)(x) }
}

impl Adjust for Op {
    type Output = Count;
    const START: i32 = 0;
    fn required(&self) {}
}

/// Advance once.
#[inline]
#[must_use = "use the next count"]
#[doc = "Hidden from the attribute facts."]
pub fn step(x: Count) -> Count { x + 1 }
pub fn make() -> fn(Count) -> Count { step }
pub const unsafe fn r#type() {}
pub extern "C" fn foreign_abi() {}

pub mod a {
    pub(in crate::a) fn restricted() {}
    pub(crate) fn shared() {}
}

extern "C" { fn foreign_item(); }

#[cfg(any())]
fn inactive() {}
#[macro_export]
macro_rules! one { () => { 1 } }
#[macro_export]
macro_rules! int { () => { i32 } }

pub fn ping() -> u32 {
    pong()
}

fn pong() -> u32 {
    1
}
