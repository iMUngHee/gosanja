#!/bin/sh
# The live check: the Rust adapter against the real rust-analyzer, in the caller's own
# environment. It needs the indexer named in fixtures/README.md, cargo with a working
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

# The checked-in indexes are what the indexer gives at the fixed path.
saved=$(mktemp -d /var/tmp/check-live.XXXXXX)
trap 'rm -rf "$saved"' EXIT
for fixture in rust-flow rust-recall; do
    cp "fixtures/$fixture/index.scip" "$saved/$fixture.scip"
done
TMPDIR=/var/tmp fixtures/regenerate.sh rust-flow rust-recall 2>/dev/null
for fixture in rust-flow rust-recall; do
    if ! cmp "$saved/$fixture.scip" "fixtures/$fixture/index.scip"; then
        cp "$saved/$fixture.scip" "fixtures/$fixture/index.scip"
        echo "check-live: fixtures/regenerate.sh gives another fixtures/$fixture/index.scip" >&2
        exit 1
    fi
done
echo "fixtures/regenerate.sh rust-flow rust-recall: both checked-in indexes reproduced byte for byte"

cargo test --locked -p gosanja-adapter-rust --test live -- --ignored --test-threads=1
