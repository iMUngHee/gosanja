#!/bin/sh
set -eu
mode=${GOSANJA_FAKE_MODE:-good}
if [ "$1" = --mode ]; then mode=$2; shift 2; fi
case "$1" in
describe)
    version=0.1
    layers='"base","syntax","calls"'
    extra=''
    case "$mode" in
        incompatible) version=1.0 ;;
        not-offered) layers='"base","calls"' ;;
        extra) extra=',"extra":true' ;;
    esac
    printf '{"contract":"%s","name":"fake","version":"1","languages":["rust"],"position_encoding":"utf-8","layers":[%s]%s}\n' "$version" "$layers" "$extra"
    ;;
index)
    [ "$2" = --root ] && [ "$4" = --out ] || exit 1
    out=$5
    if [ "$mode" = show-cache ]; then printf "%s\n" "$out" >&2; fi
    case "$mode" in
        incompatible) echo "incompatible manifest reached index" >&2; exit 88 ;;
        nonzero) exit 1 ;;
        no-result) exit 0 ;;
        not-json) echo '{' > "$out/result.json"; exit 0 ;;
        invalid-result) echo '{"contract":"0.1","layers":{"base":{"status":"invalid"}}}' > "$out/result.json"; exit 0 ;;
    esac
    builds='[{"id":"dev","features":["fast"]},{"id":"release","features":["full"]}]'
    base='"base":{"status":"ok"},'
    syntax='"syntax":{"status":"ok"},'
    extra=''
    case "$mode" in
        duplicate-builds) builds='[{"id":"dev"},{"id":"dev"}]' ;;
        path-build) builds='[{"id":"dev","target":"/tmp/build"}]' ;;
        absent-base) base='' ;;
        failed-base) base='"base":{"status":"failed"},' ;;
        no-status|not-offered) syntax='' ;;
        reported-*) syntax='"syntax":{"status":"failed","diagnostics":[{"code":"parser-failed","message":"'"$mode"' in /tmp/run-'"$$"'"}]},' ;;
        extra) extra=',"extra":true' ;;
    esac
    printf '{"contract":"0.1","layers":{%s%s"calls":{"status":"ok"}},"builds":%s%s}\n' "$base" "$syntax" "$builds" "$extra" > "$out/result.json"
    if [ "$mode" != no-index ]; then cp "$(dirname "$0")/../rust-basic/index.scip" "$out/index.scip"; fi
    if [ "$mode" = broken-scip ]; then printf "\377" > "$out/index.scip"; fi
    if [ "$mode" != no-file ]; then
        printf '{"anchor":{"file":"src/views.rs","line":2,"col":7},"visibility":"pub"%s}\n' "$extra" > "$out/syntax.jsonl"
    fi
    if [ "$mode" = bad-row ]; then printf '{"anchor":{"file":"x","line":0}}\n' >> "$out/syntax.jsonl"; fi
    case "$mode" in
        no-lf) printf '{"anchor":{"file":"x","line":0,"col":0}}' > "$out/syntax.jsonl" ;;
        crlf) printf '{"anchor":{"file":"x","line":0,"col":0}}\r\n' > "$out/syntax.jsonl" ;;
    esac
    : > "$out/calls.jsonl"
    ;;
*) exit 1 ;;
esac
