#!/bin/sh
# Maps this repository with its own engine, before and after a change, and reports what the
# change did to the code's structure: whether every snapshot still validates and which gaps
# moved, whether a dependency now runs against the layering AGENTS.md states, which module
# pairs gained references, and which definitions are left with nothing referring to them.
#
# Both sides are indexed by the engine and adapters built from the working tree, so the
# differences come from the code, not the tool. The Rust workspace is always mapped; the Go
# module under adapters/go is mapped when it changed and `go` and `scip-go` are on PATH.
#
# Usage: scripts/self-check.sh [<base>]   (default: the merge base of HEAD and main)
# Needs cargo, git and jq. Exits 1 when a snapshot does not validate, a layer failed, or a
# dependency breaks the layering; the other sections are for the change's author to read.
set -eu
cd "$(dirname "$0")/.."

for tool in cargo git jq tar; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "self-check: $tool is not on PATH" >&2
        exit 1
    fi
done
base=${1:-$(git merge-base HEAD main)}
base=$(git rev-parse --verify "$base^{commit}")
here=$PWD
jqdir=$here/scripts/self-check

work=$(mktemp -d "${TMPDIR:-/tmp}/self-check.XXXXXX")
trap 'rm -rf "$work"' EXIT
target=${CARGO_TARGET_DIR:-$here/target}
cargo build --locked --quiet -p gosanja -p gosanja-adapter-rust
gosanja=$target/debug/gosanja

mkdir "$work/base"
git archive "$base" | tar -x -C "$work/base"
echo "self-check: $(git rev-parse --short "$base") (base) against the working tree"

failed=0

# index ROOT OUT [ARGS...]: a snapshot of ROOT's base and syntax layers, validated.
index() {
    root=$1 out=$2
    shift 2
    if ! "$gosanja" index --root "$root" --out "$out" --layers base,syntax "$@" >"$out.log" 2>&1; then
        tail -n 20 "$out.log" >&2
        echo "self-check: indexing $root failed" >&2
        exit 1
    fi
    if ! "$gosanja" validate "$out" --root "$root" >"$out.validate" 2>&1; then
        cat "$out.validate" >&2
        echo "self-check: the snapshot of $root does not validate" >&2
        failed=1
    fi
}

# run_jq OUT ARGS...: jq into a file, stopping the check when jq fails, which a pipeline would
# hide from `set -e`.
run_jq() {
    out=$1
    shift
    if ! jq "$@" >"$out"; then
        echo "self-check: jq $* failed" >&2
        exit 1
    fi
}

# gaps OUT FILE: the row counts of a snapshot and the number of gaps of each kind, into FILE.
gaps() {
    run_jq "$2" -r '.counts as $c | "rows: files \($c.files), entities \($c.entities), relations \($c.relations), syntax \($c.syntax)"' "$1/manifest.json"
    run_jq "$work/gap-kinds" -r '.gap' "$1/gaps.jsonl"
    sort "$work/gap-kinds" | uniq -c | awk '{ print "gap " $2 ": " $1 }' >>"$2"
}

# noise SIDE OUT AGAIN_OUT: the indexer is not bound to give the same references twice
# (rust-analyzer on this workspace does not), so each side is indexed a second time and what
# differs between the two runs is reported as noise rather than as the change. Two runs that
# agree do not prove the indexer is deterministic; the noise comes and goes.
noise() {
    side=$1 one=$2 two=$3
    lines=0
    for table in relations gaps; do
        n=$(diff "$one/$table.jsonl" "$two/$table.jsonl" | grep -c '^[<>]' || true)
        lines=$((lines + n))
    done
    if [ "$lines" -gt 0 ]; then
        echo "noise: two runs on the $side differ in $lines relation and gap rows;"
        echo "differences of that size below are the indexer's, not the change's"
    else
        echo "noise: two runs on the $side agree"
    fi
}

# report NAME BASE_OUT HEAD_OUT ROOT
report() {
    name=$1 old=$2 new=$3 root=$4
    echo
    echo "== $name: function"
    noise base "$old" "$old.again"
    noise "working tree" "$new" "$new.again"
    gaps "$old" "$work/gaps.old"
    gaps "$new" "$work/gaps.new"
    diff -U0 "$work/gaps.old" "$work/gaps.new" | grep '^[-+][^-+]' || echo "no change in row counts or gaps"
    if jq -e 'select(.gap == "failed-layer")' "$new/gaps.jsonl" >/dev/null; then
        echo "a layer failed:"
        jq -c 'select(.gap == "failed-layer")' "$new/gaps.jsonl"
        failed=1
    fi
    if "$gosanja" diff "$old" "$new" --out "$work/$name.diff" >"$work/$name.diff.log" 2>&1; then
        jq -r '.counts | "diff: entities +\(.entities.added) -\(.entities.removed) ~\(.entities.changed), relations +\(.relations.added) -\(.relations.removed), rename candidates \(.candidates)"' \
            "$work/$name.diff/manifest.json"
    else
        cat "$work/$name.diff.log" >&2
        failed=1
    fi

    echo
    echo "== $name: modules whose references to another module grew"
    for side in old new; do
        if [ "$side" = old ]; then snap=$old; else snap=$new; fi
        run_jq "$work/edges.$side" -r --slurpfile entities "$snap/entities.jsonl" \
            -f "$jqdir/module-edges.jq" "$snap/relations.jsonl"
        sort "$work/edges.$side" | uniq -c | awk '{ print $2 "\t" $3 "\t" $1 }' | sort >"$work/modules.$side"
    done
    # Source code only: references from tests show how a module is tested, not how the code is
    # coupled. Largest growth first.
    awk -F'\t' 'NR == FNR { old[$1 "\t" $2] = $3; next }
        $1 !~ /tests/ && $2 !~ /tests/ {
            k = $1 "\t" $2; o = (k in old) ? old[k] : 0
            if ($3 > o) print ($3 - o) "\t" $1 " -> " $2 ": " o " -> " $3
        }' "$work/modules.old" "$work/modules.new" | sort -rn | cut -f2- | head -n 25 >"$work/modules.grew"
    if [ -s "$work/modules.grew" ]; then cat "$work/modules.grew"; else echo "none"; fi
    echo "module pairs in source code: $(grep -vc tests "$work/modules.old" || true) -> $(grep -vc tests "$work/modules.new" || true)"

    echo
    echo "== $name: definitions nothing refers to, new with this change"
    for side in old new; do
        if [ "$side" = old ]; then snap=$old; else snap=$new; fi
        run_jq "$work/unref.$side" -rn --slurpfile entities "$snap/entities.jsonl" \
            --slurpfile syntax "$snap/syntax.jsonl" --slurpfile relations "$snap/relations.jsonl" \
            -f "$jqdir/unreferenced.jq"
    done
    awk -F'\t' 'NR == FNR { seen[$4] = 1; next } !($4 in seen) { print $1 "\t" $2 "\t" $3 }' \
        "$work/unref.old" "$work/unref.new" >"$work/unref.added"
    if [ -s "$work/unref.added" ]; then cat "$work/unref.added"; else echo "none"; fi
    echo "unreferenced in all: $(wc -l <"$work/unref.old" | tr -d ' ') -> $(wc -l <"$work/unref.new" | tr -d ' ')"
}

# The Rust workspace.
index "$work/base" "$work/rust.old"
index "$work/base" "$work/rust.old.again"
index "$here" "$work/rust.new"
index "$here" "$work/rust.new.again"
report rust "$work/rust.old" "$work/rust.new" "$here"

echo
echo "== rust: dependencies between workspace packages"
run_jq "$work/packages.all" -r '.["key"] | split(":")[1]' "$work/rust.new/entities.jsonl"
workspace=$(sort -u "$work/packages.all" | tr '\n' ' ')
run_jq "$work/package-edges" -r --arg ws "$workspace" -f "$jqdir/package-edges.jq" \
    "$work/rust.new/relations.jsonl"
sort "$work/package-edges" | uniq -c | awk '{ print $2 " -> " $3 " (" $4 "): " $1 }' >"$work/packages"
cat "$work/packages"
# AGENTS.md: the command line uses the core and the schema, the core uses the schema, and the
# Rust adapter uses the schema and, in its tests only, the core.
allowed='gosanja -> gosanja-core
gosanja -> gosanja-schema
gosanja-core -> gosanja-schema
gosanja-adapter-rust -> gosanja-schema
gosanja-adapter-rust -> gosanja-core (test)'
while IFS= read -r line; do
    edge=${line%%:*}
    pair=${edge% (*}
    if printf '%s\n' "$allowed" | grep -qxF "$pair" || printf '%s\n' "$allowed" | grep -qxF "$edge"; then
        continue
    fi
    echo "self-check: $edge runs against the layering in AGENTS.md" >&2
    failed=1
done <"$work/packages"

# The Go module, when the change touched it.
if git diff --quiet "$base" -- adapters/go; then
    echo
    echo "== go: adapters/go did not change; not mapped"
elif ! command -v go >/dev/null 2>&1 || ! command -v scip-go >/dev/null 2>&1; then
    echo
    echo "== go: adapters/go changed, but go or scip-go is not on PATH; not mapped"
else
    for side in old new; do
        if [ "$side" = old ]; then src=$work/base; else src=$here; fi
        mkdir -p "$work/bin.$side"
        (cd "$src/adapters/go" && GOFLAGS=-mod=readonly go build -o "$work/bin.$side/gosanja-adapter-go" ./cmd/gosanja-adapter-go)
    done
    index "$work/base/adapters/go" "$work/go.old" --adapter "$work/bin.new/gosanja-adapter-go"
    index "$work/base/adapters/go" "$work/go.old.again" --adapter "$work/bin.new/gosanja-adapter-go"
    index "$here/adapters/go" "$work/go.new" --adapter "$work/bin.new/gosanja-adapter-go"
    index "$here/adapters/go" "$work/go.new.again" --adapter "$work/bin.new/gosanja-adapter-go"
    report go "$work/go.old" "$work/go.new" "$here/adapters/go"
fi

exit "$failed"
