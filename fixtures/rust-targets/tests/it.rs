const PROD_SRC: &str = include_str!("../src/a.rs");
const TEXT_SRC: &str = include_str!("../data/text_only.rs");
#[test]
fn it() {
    assert!(!PROD_SRC.is_empty() && !TEXT_SRC.is_empty());
}

const FEATURE_SRC: &str = include_str!("../src/bin/feat.rs");

pub fn feature_text() -> &'static str {
    FEATURE_SRC
}
