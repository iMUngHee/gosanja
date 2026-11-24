//! Mounts a file of the library, as an integration test may.

#[path = "../src/shared.rs"]
mod shared;

#[test]
fn shared_call_is_seven() {
    assert_eq!(shared::shared_call(), 7);
}
