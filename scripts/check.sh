#!/bin/sh
set -eu

caller_cwd=$PWD
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

# go_offline_env SCRATCH: the environment of every go command of the gate, the first ones
# included. The go command fetches a module that GONOPROXY or GOPRIVATE matches straight from
# version control even with GOPROXY=off, an empty exported value does not override the env
# file, and a go command with telemetry on can start a process that inherits all of it. So
# the go commands see a home of the gate's own under SCRATCH with telemetry off; the caller's
# env file is read once, by `go env` alone, for the module cache, GOPATH and the build cache,
# which are kept;
# and every other Go setting of the caller and of the env file is dropped (GOENV=off).
go_offline_env() {
    caller_home=${HOME:-}
    case $(uname) in
    Darwin) caller_config=$caller_home/Library/Application\ Support ;;
    *) caller_config=${XDG_CONFIG_HOME:-$caller_home/.config} ;;
    esac
    case ${GOENV:-} in
    "") env_file=$caller_config/go/env ;;
    off | /*) env_file=$GOENV ;;
    *) env_file=$caller_cwd/$GOENV ;;
    esac
    go_home=$1/home
    for config in "$go_home/.config" "$go_home/Library/Application Support"; do
        mkdir -p "$config/go/telemetry"
        echo off >"$config/go/telemetry/mode"
    done
    export CARGO_HOME="${CARGO_HOME:-$caller_home/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$caller_home/.rustup}"
    export HOME="$go_home" XDG_CONFIG_HOME="$go_home/.config"
    unset GOPRIVATE GONOPROXY GONOSUMDB GOINSECURE
    export GOFLAGS=-mod=readonly GOPROXY=off GOSUMDB=off GOTOOLCHAIN=local GOWORK=off
    gopath=$(GOENV=$env_file go env GOPATH)
    cache=$(GOENV=$env_file go env GOMODCACHE)
    build_cache=$(GOENV=$env_file go env GOCACHE)
    # Defaults the go command derived from the gate's home are the caller's.
    if [ "$gopath" = "$go_home/go" ]; then
        gopath=$caller_home/go
    fi
    if [ "$cache" = "$go_home/go/pkg/mod" ] || [ -z "$cache" ]; then
        cache=${gopath%%:*}/pkg/mod
    fi
    case $build_cache in
    "$go_home"/*)
        case $(uname) in
        Darwin) build_cache=$caller_home/Library/Caches/go-build ;;
        *) build_cache=${XDG_CACHE_HOME:-$caller_home/.cache}/go-build ;;
        esac
        ;;
    esac
    export GOPATH="$gopath" GOMODCACHE="$cache" GOCACHE="$build_cache" GOENV=off
}

if ! command -v go >/dev/null 2>&1; then
    echo "check.sh: adapters/go needs $go_needed, and there is no go on PATH" >&2
    exit 1
fi
go_scratch=$(mktemp -d)
trap 'rm -rf "$go_scratch"' EXIT

# Self-test, before the gate's own go commands: a caller with private-module settings in its
# environment and its env file, and telemetry on in its home. Every go command the setup
# starts must run in the gate's home with no private-module setting exported, the caller's
# home must stay as it was, and the settings in force afterwards must be empty.
(
    test_home=$go_scratch/caller-home
    mkdir -p "$test_home/.config/go/telemetry" "$go_scratch/bin"
    echo on >"$test_home/.config/go/telemetry/mode"
    printf 'GONOPROXY=example.invalid\nGOPRIVATE=example.invalid\n' >"$test_home/.config/go/env"
    real_go=$(command -v go)
    cat >"$go_scratch/bin/go" <<EOF
#!/bin/sh
echo "\$HOME|\${GONOPROXY-}|\${GOPRIVATE-}" >>"$go_scratch/go.log"
exec "$real_go" "\$@"
EOF
    chmod +x "$go_scratch/bin/go"
    before=$(cd "$test_home" && find . | sort)
    PATH=$go_scratch/bin:$PATH HOME=$test_home GOPRIVATE=example.invalid GONOPROXY=example.invalid
    export PATH HOME GOPRIVATE GONOPROXY
    unset XDG_CONFIG_HOME GOENV
    go_offline_env "$go_scratch/test"
    settings=$(go env GONOPROXY GOPRIVATE | tr -d '\n')
    if [ -n "$settings" ] || grep -v "^$go_scratch/test/home||\$" "$go_scratch/go.log" >&2 ||
        [ "$(cd "$test_home" && find . | sort)" != "$before" ]; then
        echo "check.sh: the gate's go commands are not isolated from the caller's Go settings" >&2
        exit 1
    fi
)
echo "go offline check: no private-module setting and no caller telemetry reach the gate's go commands"

go_offline_env "$go_scratch"
word=$(go version | awk '{ print $3 }')
if ! go_version_ok "$word"; then
    echo "check.sh: adapters/go needs $go_needed, and go version reports ${word:-nothing}" >&2
    exit 1
fi
echo "go version: $word"

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
