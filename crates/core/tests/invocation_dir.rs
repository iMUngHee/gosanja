//! A process-local working directory change must not change the described adapter.
mod support;
use gosanja_core::adapter::{self, Invocation};
use std::{env, fs};

#[test]
fn run_keeps_the_directory_and_arguments_used_for_describe() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let other = temp.path().join("other");
    let fake = support::fixture("fake-adapter/adapter.sh");
    for dir in [&first, &other] {
        fs::create_dir(dir).unwrap();
        fs::write(dir.join("record"), "").unwrap();
        fs::write(
            dir.join("adapter.sh"),
            "echo \"$3\" >> record\nGOSANJA_FAKE_MODE=good exec \"$@\"\n",
        )
        .unwrap();
    }
    let initial = env::current_dir().unwrap();
    env::set_current_dir(&first).unwrap();
    let invocation = Invocation::new(
        support::shell(),
        vec!["adapter.sh".into(), support::shell().into(), fake.into()],
    )
    .unwrap();
    let described = adapter::describe(invocation.clone()).unwrap();
    assert_eq!(described.invocation(), &invocation);
    env::set_current_dir(&other).unwrap();
    let result = described.run(&first, temp.path());
    env::set_current_dir(initial).unwrap();
    result.unwrap();
    assert_eq!(
        fs::read_to_string(first.join("record")).unwrap(),
        "describe\nindex\n"
    );
    assert_eq!(fs::read_to_string(other.join("record")).unwrap(), "");
}
