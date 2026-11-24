#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --doc
cargo deny --locked check

# The Go adapter. Nothing here downloads: a module missing from the cache fails the gate.
go_needed=go1.27.3

# go_version_ok WORD: whether the version word `go version` prints is go1.27.3, alone or with
# an `-X:<experiments>` suffix.
go_version_ok() {
    case $1 in
        "$go_needed" | "$go_needed"-X:?*) return 0 ;;
        *) return 1 ;;
    esac
}

for word in go1.27.3 go1.27.3-X:nodwarf5; do
    go_version_ok "$word" || { echo "check.sh: the version check refuses $word" >&2; exit 1; }
done
for word in go1.27.30 go1.26.3 go1.27.3-X:; do
    if go_version_ok "$word"; then
        echo "check.sh: the version check accepts $word" >&2
        exit 1
    fi
done
echo "go version check: accepts go1.27.3 and go1.27.3-X:nodwarf5, refuses go1.27.30, go1.26.3 and go1.27.3-X:"

export GOFLAGS=-mod=readonly GOPROXY=off GOTOOLCHAIN=local GOWORK=off
if ! command -v go >/dev/null 2>&1; then
    echo "check.sh: adapters/go needs $go_needed, and there is no go on PATH" >&2
    exit 1
fi
word=$(go version | awk '{ print $3 }')
if ! go_version_ok "$word"; then
    echo "check.sh: adapters/go needs $go_needed, and go version reports ${word:-nothing}" >&2
    exit 1
fi
echo "go version: $word"

# Offline whatever the caller's Go settings say: the go command fetches a module that
# GONOPROXY or GOPRIVATE matches straight from version control even with GOPROXY=off, and an
# empty exported value does not override the env file. So the module cache and GOPATH are
# read once and kept, and every other setting of the env file and the caller is dropped.
GOMODCACHE=$(go env GOMODCACHE)
GOPATH=$(go env GOPATH)
export GOMODCACHE GOPATH GOENV=off GOSUMDB=off
unset GOPRIVATE GONOPROXY GONOSUMDB GOINSECURE

unformatted=$(gofmt -l adapters/go)
if [ -n "$unformatted" ]; then
    echo "check.sh: gofmt -l adapters/go lists files that are not formatted:" >&2
    echo "$unformatted" >&2
    exit 1
fi
echo "gofmt -l adapters/go: nothing to format"

cd adapters/go
if ! go mod download; then
    echo "check.sh: the module cache lacks part of the adapter's build list; fill it once with" \
        "'go mod download' in adapters/go, which needs the network" >&2
    exit 1
fi
go vet ./...
go test -count=1 -v ./...
go mod verify
