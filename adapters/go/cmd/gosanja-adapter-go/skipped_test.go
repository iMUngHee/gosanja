package main

import (
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"testing"

	"gosanja/adapters/go/internal/contract"
)

const skipModule = "example.com/m"

// skipCase is a module of three files, p/keep.go, p/source.go and independent/ok.go, of which
// source.go is the case, run through the adapter and the engine.
type skipCase struct {
	name     string
	source   string // p/source.go; empty leaves it out
	dangling string // a file made a link to nothing
	files    map[string]string
	base     []string // "file code" under the base layer
	syntax   []string // "file code" under the syntax layer
	rows     []string // files that keep syntax rows, besides independent/ok.go
	noRows   []string // files that must have none
	narrow   []string // functions whose enclosing range the index cuts short
	leave    []string // files the index leaves out, as scip-go does
	goScript string   // replaces go when set
	check    func(t *testing.T, r contract.Result, s snapshot)
}

func manyTypeErrors() string {
	var b strings.Builder
	for i := range 14 {
		b.WriteString("var E" + string(rune('a'+i)) + " int = \"x\"\n")
	}
	return b.String()
}

func nested(n int) string {
	return "package p\nfunc step(value int) int { return value }\nfunc Deep() { " +
		strings.Repeat("step(", n) + "1" + strings.Repeat(")", n) + " }\n"
}

func TestSkipped(t *testing.T) {
	const typeError = "func Wrong() int { return \"wrong\" }\n"
	const embed = "package p\nimport _ \"embed\"\n\n"
	const missingEmbed = "//go:embed missing.txt\nvar Data string\n"
	src, keep := "p/source.go", "p/keep.go"
	cases := []skipCase{
		{name: "a syntax error after the package clause", source: "package p\nfunc Broken( { }\n",
			base: []string{src + " parse-failed"}, syntax: []string{src + " parse-failed"}, rows: []string{keep}, noRows: []string{src}},
		{name: "a broken package clause", source: "package ;\n",
			base: []string{src + " parse-failed"}, syntax: []string{src + " parse-failed"}, rows: []string{keep}, noRows: []string{src}},
		{name: "invalid UTF-8", source: "package p\n// \xff\nfunc Broken() {}\n",
			base: []string{src + " invalid-utf8"}, syntax: []string{src + " invalid-utf8"}, rows: []string{keep}, noRows: []string{src}},
		{name: "a dangling link", dangling: src,
			base: []string{src + " unreadable"}, syntax: []string{src + " unreadable"}, rows: []string{keep}},
		{name: "a type error after a line directive", source: "package p\n//line gen.y:40:1\n" + typeError,
			syntax: []string{src + " type-error"}, rows: []string{keep, src}},
		{name: "an embed pattern that matches nothing", source: embed + missingEmbed,
			syntax: []string{src + " package-error"}, rows: []string{keep, src}},
		{name: "three type errors", source: "package p\nvar First int = \"a\"\nvar Second string = 1\n" + typeError,
			syntax: []string{src + " type-error"}, rows: []string{keep, src}},
		{name: "a missing embed and a type error", source: embed + missingEmbed + typeError,
			syntax: []string{src + " package-error", src + " type-error"}, rows: []string{keep, src}},
		{name: "a line directive and a missing embed", source: embed + "//line keep.go:1\n" + missingEmbed,
			syntax: []string{keep + " package-error", src + " package-error"}, noRows: []string{keep, src}},
		{name: "a block line directive and a missing embed", source: embed + "/*line keep.go:1*/\n" + missingEmbed,
			syntax: []string{keep + " package-error", src + " package-error"}, noRows: []string{keep, src}},
		{name: "a broken clause remapped onto an unreadable file", source: "/*line a.go:1*/package !\n", dangling: "p/a.go",
			base:   []string{"p/a.go unreadable", src + " parse-failed"},
			syntax: []string{"p/a.go package-error", "p/a.go unreadable", keep + " package-error", src + " package-error", src + " parse-failed"},
			noRows: []string{keep, src}},
		{name: "a wrong argument count", source: "package p\nfunc wants(int) {}\nfunc F() { wants() }\n",
			syntax: []string{src + " type-error"}, rows: []string{keep, src}},
		{name: "a remapped redeclaration", source: "package p\n//line gen.y:10:1\nvar X int\nvar X int\n",
			syntax: []string{src + " type-error"}, rows: []string{keep, src}},
		{name: "errors past the compiler's limit", source: "package p\n//line generated.y:40:1\n" + manyTypeErrors(),
			syntax: []string{src + " type-error"}, rows: []string{keep, src}},
		{name: "type errors under two line forms", source: "package p\n//line first.y:40:1\n" + typeError + "//line other.y:3\nvar N int = \"b\"\n",
			syntax: []string{src + " type-error"}, rows: []string{keep, src}},
		{name: "build constraints", files: map[string]string{
			"tagged/only.go": "//go:build ignore\n\npackage tagged\n", "cgo/only.go": "package cgo\n\nimport \"C\"\n"},
			leave: []string{"cgo/only.go", "tagged/only.go"},
			base:  []string{"cgo/only.go build-constraint", "tagged/only.go build-constraint"}, rows: []string{keep}},
		{name: "a directory go.mod ignores", files: map[string]string{
			"go.mod": "module " + skipModule + "\n\ngo 1.26\n\nignore ./skipme\n", "skipme/x.go": "package skipme\nfunc X() {}\n"},
			leave: []string{"skipme/x.go"}, rows: []string{keep}, noRows: []string{"skipme/x.go"}},
		{name: "an unprefixed ignore at any depth", files: map[string]string{
			"go.mod": "module " + skipModule + "\n\ngo 1.26\n\nignore generated\n", "pkg/generated/a.go": "package generated\nfunc A() {}\n"},
			leave: []string{"pkg/generated/a.go"}, rows: []string{keep}, noRows: []string{"pkg/generated/a.go"}},
		{name: "an enclosing range narrower than the body", source: "package p\nfunc step(v int) int { return v }\nfunc Short() int { return step(1) }\n",
			narrow: []string{"Short"}, syntax: []string{src + " outline-outside-definition"}, rows: []string{keep, src}},
		{name: "33 nested calls", source: nested(33), syntax: []string{src + " outline-too-deep"}, rows: []string{keep, src}},
		{name: "32 nested calls", source: nested(32), rows: []string{keep, src}},
		{name: "a loader that fails as a whole",
			goScript: "for a do case $a in -compiled=true|*context.ReleaseTags*) echo 'go: no such tool' >&2; exit 1;; esac; done\nexec REALGO \"$@\"",
			check: func(t *testing.T, r contract.Result, s snapshot) {
				if e := r.Layers["syntax"]; e.Status != "failed" || len(e.Diagnostics) != 1 || e.Diagnostics[0].Code != "load-failed" {
					t.Errorf("syntax %+v", e)
				}
				found := false
				for _, g := range s.gaps {
					found = found || g["gap"] == "failed-layer" && g["layer"] == "syntax" && g["code"] == "load-failed"
				}
				if !found || s.layers["base"] != "present" {
					t.Errorf("layers %v, gaps %v", s.layers, s.gaps)
				}
			}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) { runSkipCase(t, c) })
	}
}

func runSkipCase(t *testing.T, c skipCase) {
	w := newWorld(t)
	files := map[string]string{
		"go.mod":            "module " + skipModule + "\n\ngo 1.26\n",
		"p/keep.go":         "package p\nfunc Keep() {}\n",
		"independent/ok.go": "package independent\nfunc Independent() {}\n",
	}
	if c.source != "" {
		files["p/source.go"] = c.source
	}
	for k, v := range c.files {
		files[k] = v
	}
	root := w.module("m", files)
	if c.dangling != "" {
		link(t, filepath.Join(w.dir, "nothing"), filepath.Join(root, c.dangling))
	}
	synthIndex(t, root, skipModule, c.leave, c.narrow...)
	if c.goScript != "" {
		w.fakeGo(c.goScript)
	}
	out := filepath.Join(w.dir, "snapshot")
	code, stderr := w.engine("index", "--root", root, "--out", out)
	if code != 0 {
		t.Fatalf("gosanja index exit %d: %s", code, stderr)
	}
	if code, stderr := w.engine("validate", out, "--root", root); code != 0 {
		t.Fatalf("gosanja validate exit %d: %s", code, stderr)
	}
	s := readSnapshot(t, out)
	// The adapter's own result, from a direct run on the same module.
	adapterOut, code, stderr := w.index(w.caller(nil), root)
	if code != 0 {
		t.Fatalf("adapter exit %d: %s", code, stderr)
	}
	r := result(t, adapterOut)
	if c.check != nil {
		c.check(t, r, s)
		return
	}
	for _, layer := range []string{"base", "syntax"} {
		if r.Layers[layer].Status != "ok" || s.layers[layer] != "present" {
			t.Fatalf("%s: result %+v, snapshot %s; stderr %s", layer, r.Layers[layer], s.layers[layer], stderr)
		}
	}
	listed := func(e contract.Entry) []string {
		var out []string
		for _, sk := range e.Skipped {
			out = append(out, sk.File+" "+sk.Code)
		}
		sort.Strings(out)
		return out
	}
	if got := listed(r.Layers["base"]); !equalSorted(got, c.base) {
		t.Errorf("base skipped %v, want %v", got, c.base)
	}
	if got := listed(r.Layers["syntax"]); !equalSorted(got, c.syntax) {
		t.Errorf("syntax skipped %v, want %v", got, c.syntax)
	}
	var gaps []string
	for _, b := range c.base {
		gaps = append(gaps, "base "+b)
	}
	for _, b := range c.syntax {
		gaps = append(gaps, "syntax "+b)
	}
	if got := s.skippedGaps(); !equalSorted(got, gaps) {
		t.Errorf("skipped-file gaps %v, want %v", got, gaps)
	}
	for _, f := range append([]string{"independent/ok.go"}, c.rows...) {
		if !s.rows[f] {
			t.Errorf("no syntax row for %s in the snapshot", f)
		}
	}
	for _, f := range c.noRows {
		if s.rows[f] {
			t.Errorf("a syntax row for %s in the snapshot", f)
		}
	}
	for _, g := range s.gaps {
		if g["gap"] == "failed-layer" || g["code"] == "index-incomplete" || g["code"] == "load-failed" {
			t.Errorf("gap %v", g)
		}
	}
}

func equalSorted(a, b []string) bool {
	a, b = append([]string{}, a...), append([]string{}, b...)
	sort.Strings(a)
	sort.Strings(b)
	return len(a) == len(b) && (len(a) == 0 || reflect.DeepEqual(a, b))
}

// The engine refuses a base layer that left out a file ImportDir would build.
func TestSkippedMissedFile(t *testing.T) {
	w := newWorld(t)
	root := w.module("m", map[string]string{
		"go.mod": "module " + skipModule + "\n\ngo 1.26\n", "p/keep.go": "package p\nfunc Keep() {}\n",
		"q/hidden.go": "package q\nfunc Hidden() {}\n",
	})
	synthIndex(t, root, skipModule, []string{"q/hidden.go"})
	// The go command's inventory leaves q out, as a stale or partial listing would.
	w.write(filepath.Join(w.dir, "drop", "main.go"), dropSource, 0o644)
	drop := filepath.Join(w.dir, "drop", "drop")
	w.goIn(filepath.Join(w.dir, "drop"), []string{"GOFLAGS="}, "build", "-o", drop, "main.go")
	w.fakeGo(`case "$*" in "list -e -test -deps -json="*) REALGO "$@" | '` + drop + `'; exit;; esac
exec REALGO "$@"`)
	out := filepath.Join(w.dir, "snapshot")
	code, stderr := w.engine("index", "--root", root, "--out", out)
	if code != 3 {
		t.Fatalf("gosanja index exit %d, want 3: %s", code, stderr)
	}
	if _, err := os.Stat(out); err == nil {
		if entries, _ := os.ReadDir(out); len(entries) != 0 {
			t.Errorf("a snapshot was published")
		}
	}
	adapterOut, code, stderr := w.index(w.caller(nil), root)
	if e := baseEntry(t, adapterOut); code != 0 || e.Status != "failed" || e.Diagnostics[0].Code != "index-incomplete" {
		t.Fatalf("exit %d, base %+v: %s", code, e, stderr)
	}
}

// dropSource copies a stream of JSON objects from standard input to standard output, leaving
// out the package whose import path ends in /q.
const dropSource = `package main

import (
	"encoding/json"
	"io"
	"os"
	"strings"
)

func main() {
	dec := json.NewDecoder(os.Stdin)
	for {
		var raw json.RawMessage
		if err := dec.Decode(&raw); err == io.EOF {
			return
		} else if err != nil {
			os.Exit(1)
		}
		var p struct{ ImportPath string }
		_ = json.Unmarshal(raw, &p)
		if strings.HasSuffix(p.ImportPath, "/q") {
			continue
		}
		os.Stdout.Write(append(raw, '\n'))
	}
}
`
