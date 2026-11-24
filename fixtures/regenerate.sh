#!/bin/sh
# Rebuilds the checked-in indexes of the fixtures, and the snapshots that hold the Rust
# adapter's output.
#
# Usage: fixtures/regenerate.sh [--out <dir>] [--indexes-only] [fixture...]
#   fixtures: rust-basic rust-flow rust-recall rust-calls rust-calls-variants go-basic
#             go-flow ts-basic diff-basic; with none given every fixture is rebuilt.
#
# Indexers record the absolute directory they ran in, so each fixture is copied to the fixed
# path /tmp/gosanja-fixtures/<fixture> and indexed there; build output never lands in the
# repository. That path is shared by every checkout on the machine, so a run holds the lock
# /tmp/gosanja-fixtures.lock for its whole length and stops if another run holds it.
#
# Without --out the checked-in files are replaced. With --out <dir> every file goes to the
# same relative path under <dir> and nothing under fixtures/ is written; the copies are
# indexed at the same fixed path, so the bytes are those of a normal run. --indexes-only
# makes no snapshot.
#
# The Rust indexer and the snapshots run in an environment the script makes: nothing of the
# caller's environment but PATH, the toolchain named in rust-toolchain.toml, and a home and a
# Cargo home made for the run that hold no configuration. Only a fixture's own Cargo
# configuration is in force, and the script stops if any directory above the copies, up to
# the file system's root, holds one. Override the indexer programs with RUST_ANALYZER,
# SCIP_GO and SCIP_TYPESCRIPT, and record the versions this prints in fixtures/README.md
# when you commit new indexes.
# go-flow indexes with the host build context, cgo disabled, and a warm Go module cache.
#
# The snapshots are those of rust-flow, rust-calls and both sides of diff-basic, written
# by this workspace's engine with its Rust adapter, which runs the indexer itself, and that
# of go-flow, written by the engine with the Go adapter, which runs scip-go itself. The
# engine and the adapters are built into a temporary directory; GOSANJA_BIN names a
# directory holding gosanja, gosanja-adapter-rust and gosanja-adapter-go to use instead. A
# staged Rust snapshot replaces nothing unless every scope of its manifest has the calls
# layer present and its relations hold the call relation from the Rust adapter's calls layer
# that fixtures/README.md names as the fixture's control; the go-flow snapshot replaces
# nothing unless its base and syntax layers are present. A fixture's files are put in place
# only once all of them are staged and checked, so a failed run leaves it as it was.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
repo=$(dirname "$here")
root=/tmp/gosanja-fixtures
lock=$root.lock

die() {
    echo "regenerate: $*" >&2
    exit 1
}

dest=$here
indexes_only=
while [ "$#" -gt 0 ]; do
    case $1 in
    --out)
        [ "$#" -ge 2 ] || die "--out needs a directory"
        # Resolve the destination before anything is created, so that a path into fixtures/
        # is refused without leaving a directory there.
        case $2 in
        /*) out=$2 ;;
        *) out=$(pwd)/$2 ;;
        esac
        existing=$out
        rest=
        while [ ! -d "$existing" ]; do
            rest=/$(basename "$existing")$rest
            existing=$(dirname "$existing")
        done
        case $rest/ in
        */../* | */./*) die "--out: name a directory without . or .. after its existing part" ;;
        esac
        dest=$(cd "$existing" && pwd -P)$rest
        case $dest/ in
        "$(cd "$here" && pwd -P)"/*) die "--out must lie outside $here" ;;
        esac
        mkdir -p "$dest"
        shift 2
        ;;
    --indexes-only)
        indexes_only=1
        shift
        ;;
    --*) die "unknown option: $1" ;;
    *) break ;;
    esac
done

work=$(mktemp -d)
copies=
locked=
cleanup() {
    rm -rf "$work"
    for copy in $copies; do rm -rf "${root:?}/$copy"; done
    if [ -n "$locked" ]; then rm -rf "$lock"; fi
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

if ! mkdir "$lock" 2>/dev/null; then
    holder=$(cat "$lock/pid" 2>/dev/null || echo unknown)
    die "$root is in use by process $holder; if that process is gone, remove $lock"
fi
locked=1
echo $$ >"$lock/pid"

# Cargo reads the configuration of every directory above the one it runs in.
dir=$root
while :; do
    for config in "$dir/.cargo/config" "$dir/.cargo/config.toml"; do
        if [ -e "$config" ]; then
            die "$config would be in force for the copies under $root; move it away first"
        fi
    done
    [ "$dir" != / ] || break
    dir=$(dirname "$dir")
done

rust_analyzer=${RUST_ANALYZER:-rust-analyzer}
scip_go=${SCIP_GO:-scip-go}
scip_typescript=${SCIP_TYPESCRIPT:-scip-typescript}
indexer_dir=$(dirname "$(command -v "$rust_analyzer")")
toolchain=$(sed -n 's/^channel = "\(.*\)"$/\1/p' "$repo/rust-toolchain.toml")
[ -n "$toolchain" ] || die "no channel in $repo/rust-toolchain.toml"
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
mkdir -p "$work/home" "$work/cargo-home" "$work/cache" "$work/config"
stage=$work/stage

# rust_env <command...>: runs a command in the environment made for the Rust fixtures.
rust_env() {
    env -i PATH="$indexer_dir:$PATH" HOME="$work/home" CARGO_HOME="$work/cargo-home" \
        RUSTUP_HOME="$rustup_home" RUSTUP_TOOLCHAIN="$toolchain" CARGO_TARGET_DIR="$work/target" \
        XDG_CACHE_HOME="$work/cache" XDG_CONFIG_HOME="$work/config" "$@"
}

# copy <fixture>: the fixture's sources at the fixed path, without its index or snapshots.
copy() {
    rm -rf "${root:?}/$1"
    mkdir -p "$root/$1"
    copies="$copies $1"
    (cd "$here/$1" && tar --exclude=./index.scip --exclude='./*/index.scip' --exclude=./expected \
        --exclude='./*/expected' -cf - .) | (cd "$root/$1" && tar -xf -)
    # The checkout may be read-only; the copy is the script's own and is removed at the end.
    chmod -R u+w "$root/$1"
}

# rust_index <copy directory> <staged index>
rust_index() {
    (cd "$1" && rust_env "$rust_analyzer" scip . >&2)
    mkdir -p "$(dirname "$2")"
    mv "$1/index.scip" "$2"
}

bin=
tools() {
    [ -z "$bin" ] || return 0
    if [ -n "${GOSANJA_BIN:-}" ]; then
        bin=$GOSANJA_BIN
    else
        CARGO_TARGET_DIR=$work/tools cargo build --quiet --manifest-path "$repo/Cargo.toml" \
            -p gosanja -p gosanja-adapter-rust >&2
        bin=$work/tools/debug
        (cd "$repo/adapters/go" && CGO_ENABLED=0 GOFLAGS=-mod=readonly GOWORK=off GOTOOLCHAIN=local \
            go build -o "$bin/gosanja-adapter-go" ./cmd/gosanja-adapter-go)
    fi
}

# control <snapshot path relative to fixtures/>: the control relation's two keys.
control() {
    sed -n "s#^| \`$1\` | \`\([^\`]*\)\` | \`\([^\`]*\)\` |.*#\1 \2#p" "$here/README.md"
}

# snapshot <copy directory> <staged snapshot> <snapshot path relative to fixtures/>
snapshot() {
    tools
    rust_env XDG_CACHE_HOME="$work/cache" "$bin/gosanja" index --adapter "$bin/gosanja-adapter-rust" \
        --root "$1" --out "$2" >&2
    manifest=$2/manifest.json
    scopes=$(grep -o '"layers":{[^}]*}' "$manifest" | wc -l)
    present=$(grep -o '"layers":{[^}]*"calls":"present"[^}]*}' "$manifest" | wc -l)
    if [ "$scopes" -eq 0 ] || [ "$scopes" -ne "$present" ]; then
        die "$3: the calls layer is present in $present of $scopes scopes; nothing replaced"
    fi
    keys=$(control "$3")
    [ -n "$keys" ] || die "$3: fixtures/README.md names no control relation; nothing replaced"
    from=${keys% *}
    to=${keys#* }
    if ! grep -F "{\"from\":\"$from\",\"to\":\"$to\",\"rel\":\"call\"," "$2/relations.jsonl" |
        grep -qF '"src":"adapter:rust/calls"}'; then
        die "$3: no call relation $from -> $to from adapter:rust/calls; nothing replaced"
    fi
}

# install <fixture>: moves everything staged for the fixture to the destination.
install() {
    (cd "$stage/$1" && find . -name index.scip -o -name expected -prune) | while read -r path; do
        target=$dest/$1/${path#./}
        mkdir -p "$(dirname "$target")"
        rm -rf "$target"
        mv "$stage/$1/${path#./}" "$target"
    done
}

# go_snapshot <copy directory> <staged snapshot>: the Go adapter's snapshot, in an
# environment of PATH, with scip-go's directory first, HOME and any module cache setting the
# caller exported, offline.
go_snapshot() {
    tools
    env -i PATH="$(dirname "$(command -v "$scip_go")"):$PATH" HOME="$HOME" GOPROXY=off \
        ${GOMODCACHE:+GOMODCACHE="$GOMODCACHE"} ${GOPATH:+GOPATH="$GOPATH"} \
        "$bin/gosanja" index --adapter "$bin/gosanja-adapter-go" --root "$1" --out "$2" >&2
    layers=$(grep -o '"layers":{[^}]*}' "$2/manifest.json")
    case $layers in
    *'"base":"present"'*'"syntax":"present"'*) ;;
    *) die "go-flow: the snapshot's layers are $layers, not base and syntax present; nothing replaced" ;;
    esac
}

# One Rust fixture: index, and with a snapshot path its snapshot.
rust_fixture() {
    fixture=$1
    copy "$fixture"
    rust_index "$root/$fixture" "$stage/$fixture/index.scip"
    if [ -n "${2:-}" ] && [ -z "$indexes_only" ]; then
        snapshot "$root/$fixture" "$stage/$fixture/expected" "$fixture/expected"
    fi
    install "$fixture"
    echo "$fixture: $(rust_env "$rust_analyzer" --version)"
}

# Copies of one package, each indexed in its own directory.
rust_variants() {
    copy rust-calls-variants
    for variant in "$here"/rust-calls-variants/*/; do
        name=$(basename "$variant")
        rust_index "$root/rust-calls-variants/$name" "$stage/rust-calls-variants/$name/index.scip"
    done
    install rust-calls-variants
    echo "rust-calls-variants: $(rust_env "$rust_analyzer" --version)"
}

# The two sides share the path dependencies under deps/, so the whole fixture is copied, and
# each side is indexed and then snapshotted from the same copy.
diff_basic() {
    copy diff-basic
    for side in old new; do
        rust_index "$root/diff-basic/$side" "$stage/diff-basic/$side/index.scip"
        if [ -z "$indexes_only" ]; then
            snapshot "$root/diff-basic/$side" "$stage/diff-basic/$side/expected" "diff-basic/$side/expected"
        fi
    done
    install diff-basic
    echo "diff-basic: $(rust_env "$rust_analyzer" --version)"
}

# other_index <fixture> <indexer command...>: the Go and TypeScript fixtures, in the
# caller's environment.
other_index() {
    fixture=$1
    shift
    copy "$fixture"
    (cd "$root/$fixture" && CARGO_TARGET_DIR=$work/target "$@" >&2)
    mkdir -p "$stage/$fixture"
    mv "$root/$fixture/index.scip" "$stage/$fixture/index.scip"
    install "$fixture"
    echo "$fixture: $("$1" --version)"
}

[ "$#" -gt 0 ] || set -- rust-basic rust-flow rust-recall rust-calls rust-calls-variants go-basic go-flow ts-basic diff-basic
for fixture in "$@"; do
    case $fixture in
    rust-basic | rust-recall) rust_fixture "$fixture" ;;
    rust-flow | rust-calls) rust_fixture "$fixture" snapshot ;;
    rust-calls-variants) rust_variants ;;
    diff-basic) diff_basic ;;
    go-basic) other_index go-basic "$scip_go" index --module-version v1.0.0 ;;
    go-flow)
        mkdir -p "$work/go-cache" "$work/go-tmp"
        copy go-flow
        (
            unset GOOS GOARCH GO111MODULE
            cd "$root/go-flow"
            CGO_ENABLED=0 GOFLAGS="-mod=readonly -buildvcs=false" GOENV=off GOWORK=off \
                GOTOOLCHAIN=local GOPROXY=off GOSUMDB=off GOPACKAGESDRIVER=off \
                XDG_CONFIG_HOME="$work/config" GOCACHE="$work/go-cache" GOTMPDIR="$work/go-tmp" \
                "$scip_go" index --module-version v0.0.0 --output index.scip >&2
        )
        mkdir -p "$stage/go-flow"
        mv "$root/go-flow/index.scip" "$stage/go-flow/index.scip"
        if [ -z "$indexes_only" ]; then
            go_snapshot "$root/go-flow" "$stage/go-flow/expected"
        fi
        install go-flow
        echo "go-flow: $("$scip_go" --version)"
        ;;
    ts-basic) other_index ts-basic "$scip_typescript" index --no-progress-bar ;;
    *) die "unknown fixture: $fixture" ;;
    esac
done
