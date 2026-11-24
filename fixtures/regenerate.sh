#!/bin/sh
# Rebuilds the checked-in index.scip of each fixture.
#
# Usage: fixtures/regenerate.sh [rust-basic] [rust-flow] [rust-recall] [go-basic] [ts-basic]
#                               [diff-basic]
# With no argument every fixture is rebuilt. Indexers record the absolute
# directory they ran in, so each fixture is copied to the fixed path
# /tmp/gosanja-fixtures/<fixture> and indexed there; only index.scip comes
# back, and build output never lands in the repository. Override the indexer
# programs with RUST_ANALYZER, SCIP_GO and SCIP_TYPESCRIPT, and record the
# versions this prints in fixtures/README.md when you commit new indexes.
#
# diff-basic also gets each side's snapshot, old/expected and new/expected,
# written by this workspace's engine with its Rust adapter, which runs the
# indexer itself. Both are built into a temporary directory; GOSANJA_BIN names
# a directory holding gosanja and gosanja-adapter-rust to use instead.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=/tmp/gosanja-fixtures
cargo_target=$(mktemp -d)
copies=
cleanup() {
    rm -rf "$cargo_target"
    for copy in $copies; do rm -rf "${root:?}/$copy"; done
}
trap cleanup EXIT

rust_analyzer=${RUST_ANALYZER:-rust-analyzer}
scip_go=${SCIP_GO:-scip-go}
scip_typescript=${SCIP_TYPESCRIPT:-scip-typescript}

# index <fixture> <indexer command...>
index() {
    fixture=$1
    shift
    rm -rf "${root:?}/$fixture"
    mkdir -p "$root/$fixture"
    copies="$copies $fixture"
    (cd "$here/$fixture" && tar --exclude=./index.scip --exclude=./expected -cf - .) |
        (cd "$root/$fixture" && tar -xf -)
    (cd "$root/$fixture" && CARGO_TARGET_DIR=$cargo_target "$@" >&2)
    cp "$root/$fixture/index.scip" "$here/$fixture/index.scip"
    echo "$fixture: $("$1" --version)"
}

# The two sides share the path dependencies under deps/, so the whole fixture is
# copied, and each side is indexed and then snapshotted from the same copy.
diff_basic() {
    rm -rf "${root:?}/diff-basic"
    mkdir -p "$root/diff-basic"
    copies="$copies diff-basic"
    (cd "$here/diff-basic" && tar --exclude=./expected --exclude=./old/expected \
        --exclude=./new/expected --exclude=./old/index.scip --exclude=./new/index.scip -cf - .) |
        (cd "$root/diff-basic" && tar -xf -)
    if [ -n "${GOSANJA_BIN:-}" ]; then
        bin=$GOSANJA_BIN
    else
        CARGO_TARGET_DIR=$cargo_target/tools cargo build --quiet --manifest-path "$here/../Cargo.toml" \
            -p gosanja -p gosanja-adapter-rust >&2
        bin=$cargo_target/tools/debug
    fi
    indexer_dir=$(dirname "$(command -v "$rust_analyzer")")
    # Both sides are indexed and snapshotted into a staging directory first; the checked-in
    # files are replaced only once all four exist, so a failed run leaves the fixture as it was.
    stage=$cargo_target/stage
    for side in old new; do
        side_root=$root/diff-basic/$side
        mkdir -p "$stage/$side"
        (cd "$side_root" && CARGO_TARGET_DIR=$cargo_target "$rust_analyzer" scip . >&2)
        mv "$side_root/index.scip" "$stage/$side/index.scip"
        PATH=$indexer_dir:$PATH XDG_CACHE_HOME=$cargo_target/cache \
            "$bin/gosanja" index --adapter "$bin/gosanja-adapter-rust" --root "$side_root" \
            --out "$stage/$side/expected" >&2
    done
    for side in old new; do
        rm -rf "${here:?}/diff-basic/$side/expected"
        mv "$stage/$side/expected" "$here/diff-basic/$side/expected"
        mv "$stage/$side/index.scip" "$here/diff-basic/$side/index.scip"
    done
    echo "diff-basic: $("$rust_analyzer" --version)"
}

[ "$#" -gt 0 ] || set -- rust-basic rust-flow rust-recall go-basic ts-basic diff-basic
for fixture in "$@"; do
    case $fixture in
    rust-basic | rust-flow | rust-recall) index "$fixture" "$rust_analyzer" scip . ;;
    diff-basic) diff_basic ;;
    go-basic) index go-basic "$scip_go" index --module-version v1.0.0 ;;
    ts-basic) index ts-basic "$scip_typescript" index --no-progress-bar ;;
    *)
        echo "unknown fixture: $fixture" >&2
        exit 2
        ;;
    esac
done
