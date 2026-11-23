mod model {
    pub struct Cell { pub n: u8 }
    pub trait Read { fn read(&self) -> u8; }
    impl Read for Cell { fn read(&self) -> u8 { 1 } }
    impl Cell { pub const ZERO: u8 = 0; }
}
pub use model::Cell;
use model::Read;
struct Wrap { item: Cell }
fn plain(c: Cell) -> u8 { c.read() }
fn generic<T: Read>(t: &T) -> u8 { t.read() }
fn dynamic(d: &dyn Read) -> u8 { d.read() }
pub fn run() -> String {
    let c = Cell { n: Cell::ZERO };
    let w = Wrap { item: c };
    let Cell { n: v } = w.item;
    let n = dbg!(plain(Cell { n: v }));
    format!("{}", n)
}
