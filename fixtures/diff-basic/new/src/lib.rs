pub mod util;

pub mod inner {
    pub fn meander(x: u8) -> u8 {
        x - 3
    }
}

pub fn added() -> u8 {
    2
}

pub fn new_name(x: u8) -> u8 {
    x + 1
}

pub fn uses_moved() -> u8 {
    util::mover(1) + inner::meander(9)
}

pub fn grows(x: u8) -> u8 {
    let y = x + 1;
    let z = y * 2;
    z - x
}

pub fn hidden() -> u8 {
    7
}

pub fn caller() -> u8 {
    hidden() + callee()
}

pub fn callee() -> u8 {
    5
}

pub fn uses_dep() -> u8 {
    dep::helper()
}

pub fn uses_dep2() -> u8 {
    dep2::helper()
}

pub fn width() -> usize {
    std::mem::size_of::<u64>()
}

pub fn both_g() -> u8 {
    pair1::g() + pair2::g()
}

pub fn uses_f() -> u8 {
    pair2::f()
}
