package syntax

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"unicode/utf8"

	"golang.org/x/tools/go/packages"
	"gosanja/adapters/go/internal/contract"
)

func TestSkipped(t *testing.T) {
	const file = "p/source.go"
	const good = "package p\nfunc Keep() {}"
	const typeError = "func Wrong() int { return \"wrong\" }\n"
	const embed = "package p\nimport _ \"embed\"\n\n"
	const missingEmbed = "//go:embed missing.txt\nvar Data string\n"
	const healthy = "p/keep.go"
	skip := func(name, code string) contract.Skipped { return contract.Skipped{File: name, Code: code} }

	for _, test := range []struct {
		name    string
		source  string
		missing string
		skips   []contract.Skipped
		rows    []string
	}{
		{"broken-after-clause", "package p\nfunc Broken( { }\n", "", []contract.Skipped{skip(file, "parse-failed")}, []string{healthy}},
		{"broken-package-clause", "package ;\n", "", []contract.Skipped{skip(file, "parse-failed")}, []string{healthy}},
		{"invalid-UTF8", "package p\n// \xff\nfunc Broken() {}\n", "", []contract.Skipped{skip(file, "invalid-utf8")}, []string{healthy}},
		{"unreadable", "", "p/source.go", []contract.Skipped{skip(file, "unreadable")}, []string{healthy}},
		{"line-type-error", "package p\n//line gen.y:40:1\n" + typeError, "", []contract.Skipped{skip(file, "type-error")}, []string{healthy, file}},
		{"three-type-errors", "package p\nvar First int = \"a\"\nvar Second string = 1\n" + typeError, "", []contract.Skipped{skip(file, "type-error")}, []string{healthy, file}},
		{"type-errors-two-line-forms", "package p\n//line first.y:40:1\n" + typeError + "//line other.y:3\nvar N int = \"b\"\n", "", []contract.Skipped{skip(file, "type-error")}, []string{healthy, file}},
		{"missing-embed", embed + missingEmbed, "", []contract.Skipped{skip(file, "package-error")}, []string{healthy, file}},
		{"type-error-and-embed", embed + missingEmbed + typeError, "", []contract.Skipped{skip(file, "package-error"), skip(file, "type-error")}, []string{healthy, file}},
		{"line-directive-embed", embed + "//line keep.go:1\n" + missingEmbed, "", []contract.Skipped{skip(healthy, "package-error"), skip(file, "package-error")}, nil},
		{"block-directive-embed", embed + "/*line keep.go:1*/\n" + missingEmbed, "", []contract.Skipped{skip(healthy, "package-error"), skip(file, "package-error")}, nil},
		{"argument-continuations", "package p\nfunc wants(int) {}\nfunc F() { wants() }\n", "", []contract.Skipped{skip(file, "type-error")}, []string{healthy, file}},
		{"remapped-redeclaration", "package p\n//line gen.y:10:1\nvar X int\nvar X int\n", "", []contract.Skipped{skip(file, "type-error")}, []string{healthy, file}},
		{"compiler-error-limit", "package p\n//line generated.y:40:1\n" + manyTypeErrors(), "", []contract.Skipped{skip(file, "type-error")}, []string{healthy, file}},
		{"broken-clause-remapped-onto-unreadable", "/*line a.go:1*/package !\n", "p/a.go",
			[]contract.Skipped{skip("p/a.go", "package-error"), skip("p/a.go", "unreadable"), skip(healthy, "package-error"), skip(file, "package-error"), skip(file, "parse-failed")}, nil},
	} {
		t.Run(test.name, func(t *testing.T) {
			sources := map[string]string{healthy: good, "independent/ok.go": "package independent\nfunc Independent() {}"}
			if test.source != "" {
				sources[file] = test.source
			}
			in := tempModule(t, sources, test.missing)
			proved := false
			status, rows := loadLayer(t.Context(), in, func(cfg *packages.Config, patterns ...string) ([]*packages.Package, error) {
				pkgs, err := packages.Load(cfg, patterns...)
				for _, pkg := range pkgs {
					for _, err := range pkg.Errors {
						switch test.name {
						case "broken-clause-remapped-onto-unreadable":
							pos := parsePosition(in.Root, err.Pos)
							proved = proved || pos.Line == 1 && filepath.Base(pos.Filename) == "a.go" && strings.Contains(err.Msg, "expected")
						case "compiler-error-limit":
							proved = proved || err.Kind == packages.ListError && strings.Contains(err.Msg, errorLimit)
						case "argument-continuations":
							proved = proved || err.Kind == packages.ListError && strings.Contains(err.Msg, "\n\thave ") && strings.Contains(err.Msg, "\n\twant ")
						default:
							proved = true
						}
					}
				}
				if !proved {
					for _, pkg := range pkgs {
						t.Logf("%s: %+v", pkg.ID, pkg.Errors)
					}
				}
				return pkgs, err
			})
			if !proved {
				t.Fatal("real loader did not exercise the expected diagnostic")
			}
			wantLayer(t, status, rows, test.skips, append([]string{"independent/ok.go"}, test.rows...))
		})
	}
}

func TestLoaderCommandError(t *testing.T) {
	dir := t.TempDir()
	goTool := filepath.Join(dir, "go")
	if err := os.WriteFile(goTool, []byte("#!/bin/sh\necho 'go: no such tool' >&2\nexit 1\n"), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
	in := tempModule(t, map[string]string{"p.go": "package p\nfunc F() {}"}, "")
	status, rows := loadLayer(t.Context(), in, func(cfg *packages.Config, patterns ...string) ([]*packages.Package, error) {
		pkgs, err := packages.Load(cfg, patterns...)
		if err == nil || !strings.Contains(err.Error(), "go: no such tool") {
			t.Fatalf("wanted command failure from packages.Load, got pkgs=%v err=%v", pkgs, err)
		}
		return pkgs, err
	})
	if len(rows) != 0 || !reflect.DeepEqual(status, contract.Failed("load-failed")) {
		t.Fatalf("status=%+v rows=%+v", status, rows)
	}
}

func TestCompilerMatchingUsesNearestOnce(t *testing.T) {
	root := t.TempDir()
	typed := []diagnostic{
		{parsePosition(root, "p.go:3:8"), "a message"},
		{parsePosition(root, filepath.Join(root, "p.go:3:18")), "a message"},
	}
	found := make(matcher, len(typed))
	for _, raw := range []string{"other.go:3:16", "p.go:4:16"} {
		if found.take(typed, diagnostic{parsePosition(root, raw), "a message"}, true) {
			t.Fatalf("matched wrong position %s", raw)
		}
	}
	d := diagnostic{parsePosition(root, "p.go:3:16"), "a message"}
	if found.take(typed, d, false) {
		t.Fatal("a structured copy must retain its column")
	}
	if !found.take(typed, d, true) || !found[1] || found[0] {
		t.Fatalf("column 16 must match column 18 first, got %v", found)
	}
	if !found.take(typed, d, true) || found.take(typed, d, true) {
		t.Fatalf("each type error can be consumed once, got %v", found)
	}
}

func TestCompilerBlocksWithoutMatchingTypeErrors(t *testing.T) {
	for _, block := range []string{
		"# p\np.go:2:6: unmatched\np.go:4:1: too many errors",
		"# p\n\torphaned continuation\nunpositioned compiler error",
		"# p\ntoo many errors",
	} {
		root := t.TempDir()
		fset := token.NewFileSet()
		tree := parseTree(t, fset, root, "p.go", "package p\nfunc Keep() {}")
		in := Input{Root: root, Files: map[string]File{"p.go": {}}, Documents: map[string]bool{"p.go": true}}
		status, rows := loadLayer(t.Context(), in, func(*packages.Config, ...string) ([]*packages.Package, error) {
			return []*packages.Package{{
				ID: "p", PkgPath: "p", Fset: fset, Syntax: []*ast.File{tree},
				Errors: []packages.Error{{Kind: packages.ListError, Msg: block}},
			}}, nil
		})
		wantLayer(t, status, rows, []contract.Skipped{{File: "p.go", Code: "package-error"}}, nil)
	}
}

func manyTypeErrors() string {
	var b strings.Builder
	for i := 1; i <= 14; i++ {
		fmt.Fprintf(&b, "func F%d() int { return \"s\" }\n", i)
	}
	return b.String()
}

func tempModule(t *testing.T, sources map[string]string, missing string) Input {
	t.Helper()
	const module = "example.com/diagnostics"
	in := Input{
		Root: canonicalDir(t, t.TempDir()), Env: testEnv(), Files: map[string]File{},
		Documents: map[string]bool{}, Inventory: map[string][]string{},
	}
	writeTestFile(t, in.Root, "go.mod", "module "+module+"\n\ngo 1.26\n")
	own := func(file string) {
		id := module
		if dir := filepath.Dir(file); dir != "." {
			id += "/" + filepath.ToSlash(dir)
		}
		in.Inventory[id] = append(in.Inventory[id], file)
	}
	for file, source := range sources {
		writeTestFile(t, in.Root, file, source)
		fact := File{HasLineDirective: strings.Contains(source, "//line ") || strings.Contains(source, "/*line ")}
		switch {
		case !utf8.ValidString(source):
			fact.Code = "invalid-utf8"
		default:
			if _, err := parser.ParseFile(token.NewFileSet(), file, source, 0); err != nil {
				fact.Code = "parse-failed"
			} else {
				in.Documents[file] = true
			}
		}
		in.Files[file] = fact
		own(file)
	}
	if missing != "" {
		if err := os.Symlink("does-not-exist.go", filepath.Join(in.Root, missing)); err != nil {
			t.Fatal(err)
		}
		in.Files[missing] = File{Code: "unreadable"}
		own(missing)
	}
	return in
}

func writeTestFile(t *testing.T, root, file, source string) {
	t.Helper()
	actual := filepath.Join(root, file)
	if err := os.MkdirAll(filepath.Dir(actual), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(actual, []byte(source), 0o600); err != nil {
		t.Fatal(err)
	}
}
