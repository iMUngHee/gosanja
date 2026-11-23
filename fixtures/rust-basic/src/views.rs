//! Plain-text views of a list of items.

pub fn render(items: &[&str]) -> String {
    if items.is_empty() {
        empty()
    } else {
        Joined::new(items).to_string()
    }
}

pub fn empty() -> String {
    String::from("(none)")
}

pub fn badge() -> String {
    ["🦀", &empty()].concat()
}

pub struct Joined<'a> {
    items: &'a [&'a str],
}

impl<'a> Joined<'a> {
    pub fn new(items: &'a [&'a str]) -> Self {
        Joined { items }
    }
}

impl std::fmt::Display for Joined<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.items.join(", "))
    }
}
