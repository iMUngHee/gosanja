pub mod util;

pub fn removed() -> u8 {
    1
}

pub fn old_name(x: u8) -> u8 {
    x + 1
}

pub fn mover(x: u8) -> u8 {
    x * 2
}

pub fn wander(x: u8) -> u8 {
    x - 3
}

pub fn uses_moved() -> u8 {
    mover(1) + wander(9)
}

pub fn grows(x: u8) -> u8 {
    x
}

fn hidden() -> u8 {
    7
}

pub fn caller() -> u8 {
    hidden()
}

pub fn callee() -> u8 {
    5
}

pub fn uses_dep() -> u8 {
    dep::helper()
}

pub fn both_g() -> u8 {
    pair1::g() + pair2::g()
}

pub fn uses_f() -> u8 {
    pair1::f()
}
