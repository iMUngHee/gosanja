//! Values with destructors, dropped in every way the fixture needs.

pub struct Guard(pub u32);

impl Drop for Guard {
    fn drop(&mut self) { helpers::drop(self.0) }
}

pub struct Holder {
    pub guard: Guard,
}

pub struct Pair {
    pub first: Guard,
    pub second: Guard,
}

pub fn drop_now() {
    let guard = Guard(1);
    drop(guard);
}

pub fn drop_at_end() {
    let _guard = Guard(2);
}

pub fn drop_holder() {
    let _holder = Holder { guard: Guard(3) };
}

pub fn drop_pair() {
    let _pair = Pair { first: Guard(4), second: Guard(5) };
}

mod helpers {
    pub fn drop(value: u32) { let _ = value; }
}

pub fn drop_none() {
    drop(None::<Guard>);
}

pub fn drop_some() {
    drop(Some(Guard(8)));
}

pub struct Outer(pub u32);

impl Drop for Outer {
    #[inline(always)]
    fn drop(&mut self) {
        let inner = Guard(self.0);
        drop(inner);
    }
}

pub fn drop_outer() {
    let _outer = Outer(9);
}

pub fn drop_shadowed() {
    struct Token;
    impl Drop for Token {
        fn drop(&mut self) {}
    }
    type Wrapped = Token;
    let _direct = Token;
    let inner = Token;
    {
        struct Token(Wrapped);
        let wrapper = Token(inner);
        let _ = &wrapper.0;
    }
}

pub fn drop_lookalike() {
    lookalike::mem::drop(Guard(10));
}
