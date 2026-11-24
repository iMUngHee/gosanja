//! A path dependency, not a member, whose library is named `core`.

pub mod mem {
    pub fn drop<T>(value: T) {
        let _temporary = value;
    }
}
