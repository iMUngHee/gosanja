#!/bin/sh
set -eu
mode=${GOSANJA_FAKE_MODE:-good}
name=fake
version_json='"1"'
languages='["rust"]'
layers='["base","syntax","calls"]'
encoding=utf-8
markers=''
requires=''
record=''
while [ "$#" -ge 2 ]; do
    case "$1" in
        --mode) mode=$2 ;;
        --name) name=$2 ;;
        --version-json) version_json=$2 ;;
        --languages) languages=$2 ;;
        --layers) layers=$2 ;;
        --encoding) encoding=$2 ;;
        --markers) markers=',"markers":'"$2" ;;
        --requires) requires=',"requires":'"$2" ;;
        --record) record=$2 ;;
        *) break ;;
    esac
    shift 2
done
if [ -n "$record" ]; then
    if [ "$mode" = changed-describe ] && [ "$1" = describe ] && [ -s "$record" ]; then
        mode=incompatible
    fi
    printf '%s\n' "$1" >> "$record"
fi
case "$1" in
describe)
    contract=0.2
    extra=''
    case "$mode" in
        incompatible) contract=1.0 ;;
        old-contract) contract=0.1 ;;
        describe-failed) exit 1 ;;
        invalid-manifest) echo '{}'; exit 0 ;;
        not-offered|skip-not-offered) layers='["base","calls"]' ;;
        extra) extra=',"extra":true' ;;
    esac
    printf '{"contract":"%s","name":"%s","version":%s,"languages":%s,"position_encoding":"%s","layers":%s%s%s%s}\n' "$contract" "$name" "$version_json" "$languages" "$encoding" "$layers" "$markers" "$requires" "$extra"
    if [ "$mode" = extra-stdout ]; then echo extra; fi
    ;;
index)
    [ "$2" = --root ] && [ "$4" = --out ] || exit 1
    out=$5
    if [ "$mode" = show-cache ]; then printf "%s\n" "$out" >&2; fi
    case "$mode" in
        incompatible|old-contract) echo "incompatible manifest reached index" >&2; exit 88 ;;
        nonzero) exit 1 ;;
        no-result) exit 0 ;;
        not-json) echo '{' > "$out/result.json"; exit 0 ;;
        invalid-result) echo '{"contract":"0.2","layers":{"base":{"status":"invalid"}}}' > "$out/result.json"; exit 0 ;;
    esac
    builds='[{"id":"dev","features":["fast"]},{"id":"release","features":["full"]}]'
    base='"base":{"status":"ok"},'
    syntax='"syntax":{"status":"ok"},'
    extra=''
    calls='"calls":{"status":"ok"}'
    unreadable='{"file":"src/views.rs","code":"unreadable"}'
    parse_error='{"file":"src/views.rs","code":"parse-error"}'
    case "$mode" in
        duplicate-builds) builds='[{"id":"dev"},{"id":"dev"}]' ;;
        path-build) builds='[{"id":"dev","target":"/tmp/build"}]' ;;
        skipped|skipped-reverse)
            base='"base":{"status":"ok","skipped":['"$unreadable"']},'
            entries="$unreadable,$unreadable,$parse_error"
            if [ "$mode" = skipped-reverse ]; then entries="$parse_error,$unreadable,$unreadable"; fi
            syntax='"syntax":{"status":"ok","skipped":['"$entries"']},'
            calls='"calls":{"status":"ok","skipped":[{"file":"absent.rs","code":"unreadable"}]}'
            ;;
        skip-invalid) syntax='"syntax":{"status":"failed","skipped":[{"file":"../escape","code":"bad"}]},' ;;
        skip-failed) syntax='"syntax":{"status":"failed","skipped":['"$unreadable"']},' ;;
        skip-not-offered|skip-duplicate|skip-extent) syntax='"syntax":{"status":"ok","skipped":['"$unreadable"']},' ;;
        failed-base-message) base='"base":{"status":"failed","diagnostics":[{"code":"lockfile-missing","message":"missing project lockfile"},{"code":"other-code"}]},' ;;
        failed-base-code) base='"base":{"status":"failed","diagnostics":[{"code":"lockfile-missing"},{"code":"other-code"}]},' ;;
        absent-base) base='' ;;
        failed-base) base='"base":{"status":"failed"},' ;;
        no-status|not-offered) syntax='' ;;
        reported-*) syntax='"syntax":{"status":"failed","diagnostics":[{"code":"parser-failed","message":"'"$mode"' in /tmp/run-'"$$"'"}]},' ;;
        extra) extra=',"extra":true' ;;
    esac
    printf '{"contract":"0.2","layers":{%s%s%s},"builds":%s%s}\n' "$base" "$syntax" "$calls" "$builds" "$extra" > "$out/result.json"
    if [ "$mode" != no-index ]; then cp "$(dirname "$0")/../rust-basic/index.scip" "$out/index.scip"; fi
    if [ "$mode" = broken-scip ]; then printf "\377" > "$out/index.scip"; fi
    if [ "$mode" != no-file ]; then
        printf '{"anchor":{"file":"src/views.rs","line":2,"col":7},"visibility":"pub"%s}\n' "$extra" > "$out/syntax.jsonl"
    fi
    if [ "$mode" = bad-row ]; then printf '{"anchor":{"file":"x","line":0}}\n' >> "$out/syntax.jsonl"; fi
    case "$mode" in
        skip-duplicate) printf '{"anchor":{"file":"src/views.rs","line":2,"col":7}}\n' >> "$out/syntax.jsonl" ;;
        skip-extent) printf '{"anchor":{"file":"src/views.rs","line":2,"col":7},"body":[2,0,8,1],"outline":[{"k":"opaque","range":[0,0,0,2]}]}\n' > "$out/syntax.jsonl" ;;
        nameless-call) printf '{"anchor":{"file":"src/views.rs","line":2,"col":7},"outline":[{"k":"call","range":[4,8,4,15]}]}\n' > "$out/syntax.jsonl" ;;
        no-lf) printf '{"anchor":{"file":"x","line":0,"col":0}}' > "$out/syntax.jsonl" ;;
        crlf) printf '{"anchor":{"file":"x","line":0,"col":0}}\r\n' > "$out/syntax.jsonl" ;;
    esac
    : > "$out/calls.jsonl"
    ;;
*) exit 1 ;;
esac
