//! The library of the calls fixture.
mod drops;

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use calls_derive::Describe;

pub use drops::{Guard, Holder, Outer, Pair, drop_at_end, drop_holder, drop_none, drop_now, drop_outer, drop_lookalike, drop_pair, drop_shadowed, drop_some};

pub fn leaf() -> u32 {
    1
}

pub fn direct() -> u32 {
    leaf()
}

pub fn generic<T: Copy>(x: T) -> T {
    x
}

pub fn two_types() -> (u32, bool) {
    (generic(1u32), generic(true))
}

pub fn twice<T>(x: T) -> T {
    leaf();
    x
}

pub fn use_twice() -> (u8, char) {
    (twice(1u8), twice('c'))
}

pub fn make_adder(n: u32) -> impl Fn(u32) -> u32 {
    move |x| {
        leaf();
        x ^ n
    }
}

pub fn use_adder() -> u32 {
    make_adder(1)(2)
}

pub async fn ready() -> u32 {
    2
}

pub async fn after_await() -> u32 {
    let _ = ready().await;
    leaf()
}

pub fn run_after_await() -> u32 {
    let mut future = pin!(after_await());
    let waker = Waker::noop();
    match future.as_mut().poll(&mut Context::from_waker(waker)) {
        Poll::Ready(value) => value,
        Poll::Pending => 0,
    }
}

#[inline(always)]
pub fn always() -> u32 {
    leaf()
}

pub fn calls_always() -> u32 {
    always()
}

pub trait Shape {
    fn area(&self) -> u32;
}

pub struct Square;

impl Shape for Square {
    fn area(&self) -> u32 {
        leaf()
    }
}

pub fn dyn_call(shape: &dyn Shape) -> u32 {
    shape.area()
}

pub fn use_dyn() -> u32 {
    dyn_call(&Square)
}

#[derive(Describe)]
pub struct Named;

pub fn use_derive() -> &'static str {
    Named::describe()
}

macro_rules! make_fn {
    ($name:ident) => {
        pub fn $name() -> u32 {
            $crate::leaf()
        }
    };
}

pub mod real { pub fn made() -> u32 { super::leaf() } } pub mod made_by_macro { make_fn!(made); }

pub fn use_made() -> (u32, u32) {
    (real::made(), made_by_macro::made())
}

pub fn fill<const N: usize>() -> [u32; N] {
    [leaf(); N]
}

pub fn use_fill() -> [u32; 2] {
    fill::<2>()
}

pub struct Wrapper<T>(pub T);

impl<T> Wrapper<T> {
    pub fn get(self) -> T {
        leaf();
        self.0
    }
}

pub fn use_wrapper() -> u8 {
    Wrapper(1u8).get()
}

#[unsafe(no_mangle)]
pub extern "C" fn shared_entry() -> u32 {
    leaf()
}

pub fn
split() -> u32 {
    leaf()
}

pub fn use_split() -> u32 {
    split()
}

pub fn accented() -> u32 { let _text = "été 😀"; leaf() }

impl Wrapper<i16> {
    pub fn concrete(&self) -> u32 {
        leaf()
    }
}

pub fn use_impls() -> (u32, i16) {
    (Wrapper(2i16).concrete(), Wrapper(3i16).get())
}

pub trait Greeter {
    fn greet(&self) -> u32 {
        leaf()
    }
}

pub struct Hello;

impl Greeter for Hello {}

pub fn use_greet() -> u32 {
    Hello.greet()
}

pub mod first { pub fn same() -> u32 { super::leaf() } } pub mod second { pub fn
same() -> u32 {
    super::leaf() } }

pub fn use_same() -> (u32, u32) {
    (first::same(), second::same())
}

macro_rules! adder {
    () => {
        |x: u32| {
            $crate::leaf();
            x
        }
    };
}

pub fn macro_closure() -> u32 {
    let add = adder!();
    add(1)
}

pub struct Engine;

#[calls_derive::proxy]
impl Engine {
    pub fn run(&self) -> u32 {
        leaf()
    }

    pub fn beside(&self) -> u32 {
        leaf()
    }
}

pub fn proxy_helper() -> u32 {
    3
}

pub fn use_proxy() -> (u32, u32, u32) {
    (
        Engine.run(),
        EngineProxy.run(),
        Engine.beside(),
    )
}

pub struct Gen;

#[calls_derive::proxy]
impl Gen {
    pub fn run<T>(&self) -> u32 {
        leaf()
    }
}

pub fn use_gen_proxy() -> u32 {
    GenProxy.run()
}

pub struct Slot<T>(pub T);

#[calls_derive::retype(u16)]
impl Slot<u8> {
    pub fn run(&self) -> u32 {
        leaf()
    }
}

pub fn use_slots() -> (u32, u32) {
    (Slot(1u8).run(), Slot(2u16).run())
}

pub trait Tagged {
    fn tag(&self) -> u32;
}

impl<T> Tagged for T {
    fn tag(&self) -> u32 {
        leaf()
    }
}

pub fn use_blanket() -> u32 {
    5u8.tag()
}

pub struct Aliased;

pub type Alias = Aliased;

impl Alias {
    pub fn via_alias(&self) -> u32 {
        leaf()
    }
}

pub fn use_alias() -> u32 {
    Aliased.via_alias()
}

pub mod plain;

#[cfg(test)]
#[path = "mounted.rs"]
pub mod under_test;

#[cfg(not(test))]
#[path = "mounted.rs"]
pub mod in_build;

#[cfg(test)]
pub use under_test as mounted_module;

#[cfg(not(test))]
pub use in_build as mounted_module;

pub fn use_mounted() -> (u32, u32) {
    (mounted_module::mounted(), plain::plain())
}

pub fn get_two() -> (u8, i16) {
    (Wrapper(1u8).get(), Wrapper(2i16).get())
}

#[cfg(calls_flag)]
pub fn only_with_flag() -> u32 {
    leaf()
}

pub fn use_flag() -> u32 {
    only_with_flag()
}

pub fn two_on_one_line() -> (u32, u32) {
    (leaf(), leaf())
}

pub struct Meters(pub u32);

impl std::ops::Add for Meters {
    type Output = Meters;

    fn add(self, other: Meters) -> Meters {
        Meters(self.0 ^ other.0)
    }
}

pub fn use_operator() -> Meters {
    Meters(1) + Meters(2)
}

pub fn use_shared_entry() -> u32 {
    shared_entry()
}

#[cfg(test)]
pub fn only_in_test() -> u32 {
    leaf()
}

#[cfg(test)]
pub fn call_only_in_test() -> u32 {
    only_in_test()
}

#[cfg(not(test))]
pub fn only_outside_test() -> u32 {
    leaf()
}

#[cfg(not(test))]
pub fn call_only_outside_test() -> u32 {
    only_outside_test()
}

pub fn value_and_call() -> (fn() -> u32, u32) {
    (leaf, direct())
}

/* é 😀 */ pub fn after_accent() -> u32 { leaf() }

pub fn use_after_accent() -> u32 {
    after_accent()
}

pub fn chained() -> u8 {
    Wrapper(4u8)
        .get()
}

pub fn split_path() -> u32 {
    crate::
        leaf()
}

pub mod a {
    pub trait Task {
        fn work(&self) -> u32;
    }
}

pub mod b {
    pub trait Task {
        fn work(&self) -> u32;
    }
}

pub struct Local;

#[calls_derive::retrait(b::Task)]
impl a::Task for Local {
    fn work(&self) -> u32 {
        leaf()
    }
}

#[cfg(test)]
use a::Task as Chosen;
#[cfg(not(test))]
use b::Task as Chosen;

pub fn use_chosen() -> u32 {
    Chosen::work(&Local)
}

#[cfg(test)] pub fn alt() -> u32 { leaf() } #[cfg(not(test))] pub fn alt() -> u32 { direct() }

pub fn use_alt() -> u32 {
    alt()
}

pub fn apply(f: fn() -> u32) -> u32 {
    f()
}

pub fn pass_by_name() -> u32 {
    apply(leaf)
}

pub mod shared;

pub fn use_shared() -> u32 {
    shared::shared_call()
}

#[calls_derive::keep]
pub fn kept() -> u32 {
    leaf()
}

pub struct Kept;

#[calls_derive::keep]
impl Kept {
    pub fn kept_method(&self) -> u32 {
        leaf()
    }
}

pub fn plain_beside() -> u32 {
    leaf()
}

pub fn use_kept() -> (u32, u32, u32) {
    (
        kept(),
        Kept.kept_method(),
        plain_beside(),
    )
}

mod spelled {
    use calls_derive::keep as test;

    pub struct ByTest;

    #[test]
    impl ByTest {
        pub fn by_test(&self) -> u32 {
            crate::leaf()
        }
    }
}

pub mod clippy {
    pub use calls_derive::keep as skip;
}

pub struct ByClippy;

#[clippy::skip]
impl ByClippy {
    pub fn by_clippy(&self) -> u32 {
        leaf()
    }
}

pub fn use_spelled() -> (u32, u32) {
    (spelled::ByTest.by_test(), ByClippy.by_clippy())
}

#[cfg_attr(feature = "fast", inline)]
pub fn maybe_inline() -> u32 {
    leaf()
}

#[cfg_attr(feature = "off", calls_derive::keep)]
pub fn maybe_kept() -> u32 {
    leaf()
}

pub fn use_cfg_attr() -> (u32, u32) {
    (maybe_inline(), maybe_kept())
}

pub trait First {
    fn go(&self) -> u32;
}

pub trait Second {
    fn go(&self) -> u32;
}

pub struct Shared;

impl First for Shared {
    fn go(&self) -> u32 {
        (|| (leaf)())() } } impl Second for Shared { fn go(&self) -> u32 { (|| leaf())() }
}

pub struct Apart;

impl First for Apart {
    fn go(&self) -> u32 {
        (|| leaf())()
    }
}

impl Second for Apart {
    fn go(&self) -> u32 {
        (|| leaf())()
    }
}

pub struct Joined;

impl First for Joined {
    fn go(&self) -> u32 {
        (leaf)() } } impl Second for Joined { fn go(&self) -> u32 { leaf() }
}

pub fn use_go() -> (u32, u32, u32, u32, u32, u32) {
    (
        First::go(&Shared),
        Second::go(&Shared),
        First::go(&Apart),
        Second::go(&Apart),
        First::go(&Joined),
        Second::go(&Joined),
    )
}

pub struct Original;

pub struct Copied;

#[calls_derive::copy_to(Copied)]
impl Original {
    pub fn work(&self) -> u32 { helper() }
}

pub fn helper() -> u32 {
    leaf()
}

pub fn use_copies() -> (u32, u32) {
    (Original.work(), Copied.work())
}

#[inline]
pub fn hinted() -> u32 {
    leaf()
}

#[unsafe(export_name = "_Read")]
pub extern "C" fn read_value() -> u32 {
    leaf()
}

macro_rules! save {
    ($call:expr) => {
        let _ = $call;
        macro_rules! saved {
            () => {
                $call
            };
        }
    };
}

pub fn saver() -> u32 {
    save!(leaf());
    fn inner() -> u32 {
        let evaluate = || {
            saved!()
        };
        evaluate()
    }
    let value = inner();
    value ^ direct()
}

pub mod nested_run {
    pub fn run() -> u32 {
        #[cfg(not(test))]
        fn run() -> u32 {
            (|| crate::leaf())() } let first = crate::leaf();
        let other: fn() -> u32 = run;
        first ^ other()
    }
}

pub mod const_run {
    pub fn run() -> u32 {
        fn nested() -> u32 {
            #[allow(non_upper_case_globals)]
            const run: fn() -> u32 = || crate::leaf(); run() } let first = crate::leaf();
        let other: fn() -> u32 = nested;
        first ^ other()
    }
}

pub mod bom;

pub mod templated {
    pub fn run() -> u32 {
        let first = crate::leaf(); macro_rules! closure_of { () => { || crate::leaf() } }
        let second = crate::leaf();
        mod inner {
            pub fn run() -> u32 {
                (closure_of!())()
            }
        }
        first ^ second ^ inner::run()
    }
}

pub fn local_derive() -> u32 {
    #[derive(Clone, Copy)]
    struct Point { value: u32 } let first = leaf();
    let second = leaf();
    first ^ second ^ Point { value: 3 }.value
}

pub struct Panel;

#[derive(calls_derive::Label)] pub struct Plate; impl Panel { pub fn label() -> u32 { leaf() } }

pub struct Sheet;

#[derive(calls_derive::Label)]
pub struct Board {
    pub size: u32 } impl Sheet { pub fn label() -> u32 { leaf() } }

pub struct Card;

impl Card {
    pub fn label() -> u32 {
        leaf()
    }
}

#[cfg(test)]
pub type PickPlate = Panel;
#[cfg(not(test))]
pub type PickPlate = Plate;

#[cfg(test)]
pub type PickBoard = Sheet;
#[cfg(not(test))]
pub type PickBoard = Board;

pub fn use_labels() -> (u32, u32, u32) {
    (
        PickPlate::label(),
        PickBoard::label(),
        Card::label(),
    )
}
