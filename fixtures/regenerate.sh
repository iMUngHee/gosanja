#!/bin/sh
# Rebuilds the checked-in index.scip of each fixture.
#
# Usage: fixtures/regenerate.sh [rust-basic] [rust-flow] [rust-recall] [go-basic] [ts-basic]
# With no argument every fixture is rebuilt. Indexers record the absolute
# directory they ran in, so each fixture is copied to the fixed path
# /tmp/gosanja-fixtures/<fixture> and indexed there; only index.scip comes
# back, and build output never lands in the repository. Override the indexer
# programs with RUST_ANALYZER, SCIP_GO and SCIP_TYPESCRIPT, and record the
# versions this prints in fixtures/README.md when you commit new indexes.
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

[ "$#" -gt 0 ] || set -- rust-basic rust-flow rust-recall go-basic ts-basic
for fixture in "$@"; do
    case $fixture in
    rust-basic | rust-flow | rust-recall) index "$fixture" "$rust_analyzer" scip . ;;
    go-basic) index go-basic "$scip_go" index --module-version v1.0.0 ;;
    ts-basic) index ts-basic "$scip_typescript" index --no-progress-bar ;;
    *)
        echo "unknown fixture: $fixture" >&2
        exit 2
        ;;
    esac
done
