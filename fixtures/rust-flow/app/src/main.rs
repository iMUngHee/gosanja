#![allow(dead_code)]

use flow_kit::{make, Op};
mod extra;

macro_rules! marker { () => { fn generated() {} }; }
marker! {}

struct Shell;
impl Shell {
    marker! {}
}

const INNER_VALUE: i32 = 2;
fn outer(x: i32) -> i32 { x }

async fn flow(opt: Option<i32>, op: Op) -> Option<i32> {
    fn nested() { outer(0); }
    macro_rules! item { () => { struct Hidden; }; }
    item! {}
    let Some(x) = opt else { return None };
    if x > 0 {} else if let Some(_) = opt {}
    match x { n if n > 0 => (), _ => () }
    loop { break; }
    while false { continue; }
    while let Some(_) = None::<i32> { break; }
    for _ in [x] {}
    let _ = opt?;
    let f = |v| outer(v);
    let _ = async { x }.await;
    let _ = async move { x }.await;
    let _ = /*é*/ flow_kit::step(x);
    op.apply(x);
    outer(INNER_VALUE);
    make()(x);
    (op.f)(x);
    let _ = stringify!(x);
    assert!(true); let _: flow_kit::int!() = match x { flow_kit::one!() => 1, _ => 0 };
    Some(f(x))
}

fn main() {}
