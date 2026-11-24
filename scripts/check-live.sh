#!/bin/sh
# The live check: the Rust adapter against the real rust-analyzer. The fixtures are
# regenerated in the environment fixtures/regenerate.sh makes; the live tests run in the
# caller's own. It needs the indexer named in fixtures/README.md, cargo with a working
# compiler and linker, rustup with its proxies, and inotifywait (inotify-tools); it installs
# nothing. Build and output directories go under /var/tmp.
#
# Usage: scripts/check-live.sh
set -eu
cd "$(dirname "$0")/.."

for tool in rust-analyzer cargo rustc rustup inotifywait; do
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

pinned=$(sed -n 's/.*`\(rust-analyzer [^`]*\)`.*/\1/p' fixtures/README.md | sort -u)
if [ "$indexer" != "$pinned" ]; then
    echo "check-live: the indexer is '$indexer'; fixtures/README.md names '$pinned'" >&2
    exit 1
fi

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
