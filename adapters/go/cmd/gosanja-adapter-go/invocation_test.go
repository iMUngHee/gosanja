package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"testing"

	adaptergo "gosanja/adapters/go"
)

func TestDescribe(t *testing.T) {
	w := newWorld(t)
	var stdout, stderr strings.Builder
	if code := run([]string{"describe"}, w.caller(nil), runtime.GOOS, &stdout, &stderr); code != 0 {
		t.Fatalf("exit %d: %s", code, stderr.String())
	}
	validate(t, "adapter-manifest.schema.json", []byte(stdout.String()))
	var got map[string]any
	if err := json.Unmarshal([]byte(stdout.String()), &got); err != nil {
		t.Fatal(err)
	}
	version, err := os.ReadFile("../../VERSION")
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]any{
		"contract": "0.5", "name": "go", "version": strings.TrimSuffix(string(version), "\n"),
		"languages": []any{"go"}, "position_encoding": "utf-8", "layers": []any{"base", "syntax", "build"},
		"requires": []any{map[string]any{"tool": "go"}, map[string]any{"tool": "scip-go"}},
		"markers":  []any{"go.mod"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("manifest\n got %v\nwant %v", got, want)
	}
	if adaptergo.Version() != want["version"] {
		t.Fatalf("Version() %q", adaptergo.Version())
	}
}

func TestInvocation(t *testing.T) {
	w := newWorld(t)
	root := w.flow("go-flow")
	file := filepath.Join(w.dir, "a-file")
	w.write(file, "x", 0o644)
	full := filepath.Join(w.dir, "full")
	w.write(filepath.Join(full, "x"), "x", 0o644)
	beneath := filepath.Join(root, "out")
	if err := os.Mkdir(beneath, 0o755); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(w.dir, "link-into-root")
	linkTarget := filepath.Join(root, "linked-out")
	if err := os.Mkdir(linkTarget, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(linkTarget, link); err != nil {
		t.Fatal(err)
	}
	empty := func() string {
		d, err := os.MkdirTemp(w.dir, "empty-")
		if err != nil {
			t.Fatal(err)
		}
		return d
	}
	cases := map[string][]string{
		"no root":           {"index", "--out", empty()},
		"root missing":      {"index", "--root", filepath.Join(w.dir, "none"), "--out", empty()},
		"no out":            {"index", "--root", root},
		"out missing":       {"index", "--root", root, "--out", filepath.Join(w.dir, "none")},
		"out not empty":     {"index", "--root", root, "--out", full},
		"out a file":        {"index", "--root", root, "--out", file},
		"out the root":      {"index", "--root", root, "--out", root},
		"out beneath":       {"index", "--root", root, "--out", beneath},
		"out through link":  {"index", "--root", root, "--out", link},
		"unknown option":    {"index", "--root", root, "--out", empty(), "--fast", "1"},
		"option twice":      {"index", "--root", root, "--root", root, "--out", empty()},
		"no command":        {},
		"unknown command":   {"lint"},
		"describe and more": {"describe", "now"},
	}
	for _, list := range []string{"syntax", "base,base", "base,graph", "base,", "base, syntax", ""} {
		cases["layers "+list] = []string{"index", "--root", root, "--out", empty(), "--layers", list}
	}
	before := listing(t, w.dir)
	for name, args := range cases {
		code, stderr := w.run(w.caller(nil), runtime.GOOS, args...)
		if code != 2 || !strings.Contains(stderr, "usage:") {
			t.Errorf("%s: exit %d, stderr %q", name, code, stderr)
		}
	}
	if after := listing(t, w.dir); !reflect.DeepEqual(before, after) {
		t.Errorf("an invalid invocation wrote something")
	}
	if indexRuns(t, w) != 0 {
		t.Errorf("an invalid invocation ran the indexer")
	}
	t.Run("out through a case alias", func(t *testing.T) {
		alias, ok := caseAlias(t, w, root)
		if !ok {
			t.Skip("the temporary volume is case-sensitive: this case needs a case-insensitive volume")
		}
		code, stderr := w.run(w.caller(nil), runtime.GOOS, "index", "--root", root, "--out", alias)
		if code != 2 || !strings.Contains(stderr, "usage:") {
			t.Errorf("exit %d, stderr %q", code, stderr)
		}
		if indexRuns(t, w) != 0 {
			t.Errorf("the indexer ran")
		}
	})

	for _, order := range [][]string{
		{"--layers", "base", "--root", root, "--out", "OUT"},
		{"--root", root, "--layers", "base", "--out", "OUT"},
		{"--root", root, "--out", "OUT", "--layers", "base"},
	} {
		out := empty()
		args := []string{"index"}
		for _, a := range order {
			if a == "OUT" {
				a = out
			}
			args = append(args, a)
		}
		if code, stderr := w.run(w.caller(nil), runtime.GOOS, args...); code != 0 {
			t.Errorf("%v: exit %d: %s", order, code, stderr)
		}
		if baseEntry(t, out).Status != "ok" {
			t.Errorf("%v: base %+v", order, baseEntry(t, out))
		}
	}
}

func TestInvocationUnwritableOutput(t *testing.T) {
	if os.Geteuid() == 0 {
		t.Fatal("this test needs an unprivileged user: the super-user can write into a 0555 directory")
	}
	w := newWorld(t)
	root := w.flow("go-flow")
	out := filepath.Join(w.dir, "locked")
	if err := os.Mkdir(out, 0o555); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chmod(out, 0o755) })
	code, stderr := w.run(w.caller(nil), runtime.GOOS, "index", "--root", root, "--out", out)
	if code != 1 || stderr == "" {
		t.Fatalf("exit %d, stderr %q", code, stderr)
	}
}

// caseAlias gives a spelling of a directory beneath root through a case alias of the root,
// when the temporary volume is case-insensitive, and reports false otherwise.
func caseAlias(t *testing.T, w *world, root string) (string, bool) {
	t.Helper()
	probe := filepath.Join(w.dir, "case-probe-a")
	if err := os.Mkdir(probe, 0o755); err != nil {
		t.Fatal(err)
	}
	defer os.Remove(probe)
	if _, err := os.Stat(filepath.Join(w.dir, "CASE-PROBE-A")); err != nil {
		return "", false
	}
	alias := filepath.Join(filepath.Dir(root), strings.ToUpper(filepath.Base(root)), "alias-out")
	if err := os.Mkdir(filepath.Join(root, "alias-out"), 0o755); err != nil {
		t.Fatal(err)
	}
	return alias, true
}

func TestTrailingOptionWithoutValue(t *testing.T) {
	w := newWorld(t)
	code, stderr := w.run(w.caller(nil), runtime.GOOS, "index", "--root", w.dir, "--out")
	if code != 2 || !strings.Contains(stderr, "--out needs a value") {
		t.Fatalf("exit %d, stderr %q", code, stderr)
	}
}
