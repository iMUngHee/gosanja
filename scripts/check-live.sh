#!/bin/sh
# The live check: the Rust adapter against the real rust-analyzer, and the Go adapter against
# the real scip-go and go. The fixtures are regenerated in the environment
# fixtures/regenerate.sh makes; the live tests run in the caller's own. It needs the indexers
# and the Go version named in fixtures/README.md, cargo with a working compiler and linker,
# rustup with its proxies, and inotifywait (inotify-tools); it installs nothing. Build and
# output directories go under /var/tmp.
#
# Usage: scripts/check-live.sh
set -eu
cd "$(dirname "$0")/.."

for tool in rust-analyzer cargo rustc rustup inotifywait go scip-go; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "check-live: $tool is not on PATH; this check needs it and installs nothing" >&2
        exit 1
    fi
done

indexer=$(rust-analyzer --version 2>/dev/null)
echo "rust-analyzer: $indexer"
echo "cargo: $(cargo --version)"
echo "rustc: $(rustc --version)"
echo "rustup: $(rustup --version 2>/dev/null)"
echo "inotifywait: $(inotifywait --help 2>&1 | head -n 1)"
scip_go=$(scip-go --version 2>/dev/null)
go_word=$(go version | awk '{ print $3 }')
echo "scip-go: $scip_go"
echo "go: $(go version)"

pinned=$(sed -n 's/.*`\(rust-analyzer [^`]*\)`.*/\1/p' fixtures/README.md | sort -u)
if [ "$indexer" != "$pinned" ]; then
    echo "check-live: the indexer is '$indexer'; fixtures/README.md names '$pinned'" >&2
    exit 1
fi

# fixtures/README.md names scip-go 0.2.10 built with Go 1.27.3; a version word may carry an
# -X:<experiments> suffix.
if [ "$scip_go" != 0.2.10 ]; then
    echo "check-live: scip-go is '$scip_go'; fixtures/README.md names 0.2.10" >&2
    exit 1
fi
case $go_word in
go1.27.3 | go1.27.3-X:?*) ;;
*)
    echo "check-live: go is '$go_word'; fixtures/README.md names Go 1.27.3" >&2
    exit 1
    ;;
esac

# The checked-in indexes and snapshots are what a fresh run gives at the fixed path. The run
# writes into a scratch destination and this script compares; nothing under fixtures/ is
# written.
fresh=$(mktemp -d /var/tmp/check-live.XXXXXX)
trap 'rm -rf "$fresh"' EXIT
fixtures="rust-basic rust-flow rust-recall rust-calls rust-calls-variants diff-basic"
# shellcheck disable=SC2086 # the fixture names are words
if ! TMPDIR=/var/tmp fixtures/regenerate.sh --out "$fresh/out" $fixtures 2>"$fresh/log"; then
    tail -n 20 "$fresh/log" >&2
    echo "check-live: fixtures/regenerate.sh failed" >&2
    exit 1
fi
differs=
for path in $(cd "$fresh/out" && find . \( -name index.scip -o -name expected \) -prune -print | sort); do
    path=${path#./}
    if ! diff -r "$fresh/out/$path" "fixtures/$path" >"$fresh/diff" 2>&1; then
        echo "check-live: a fresh run gives another fixtures/$path:" >&2
        head -n 20 "$fresh/diff" >&2
        differs=1
    fi
done
if [ -n "$differs" ]; then
    exit 1
fi
echo "fixtures/regenerate.sh $fixtures: every checked-in index and snapshot reproduced byte for byte"

cargo test --locked -p gosanja-adapter-rust --test live -- --ignored --test-threads=1

# go-flow: scip-go's index bytes vary from run to run, so a fresh run is checked by its
# snapshot, not by its index.
if ! TMPDIR=/var/tmp fixtures/regenerate.sh --out "$fresh/go" go-flow 2>"$fresh/log"; then
    tail -n 20 "$fresh/log" >&2
    echo "check-live: fixtures/regenerate.sh go-flow failed" >&2
    exit 1
fi
if ! diff -r "$fresh/go/go-flow/expected" fixtures/go-flow/expected >"$fresh/diff" 2>&1; then
    echo "check-live: a fresh run gives another fixtures/go-flow/expected:" >&2
    head -n 20 "$fresh/diff" >&2
    exit 1
fi
echo "fixtures/regenerate.sh go-flow: the checked-in snapshot reproduced byte for byte"

(cd adapters/go && GOFLAGS=-mod=readonly GOPROXY=off GOTOOLCHAIN=local GOWORK=off TMPDIR=/var/tmp \
    go test -tags live -count=1 -v -run '^TestLive' ./cmd/gosanja-adapter-go)
