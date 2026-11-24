package syntax

import (
	"bytes"
	"encoding/json"
	"errors"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"

	"golang.org/x/tools/go/packages"
	"gosanja/adapters/go/internal/contract"
)

func testEnv() []string {
	return append(os.Environ(), "GOENV=off", "GOFLAGS=-mod=readonly -buildvcs=false",
		"GOTOOLCHAIN=local", "GOWORK=off", "GOPACKAGESDRIVER=off", "GOPROXY=off",
		"GOSUMDB=off", "CGO_ENABLED=0")
}

func TestItems(t *testing.T) {
	t.Run("fixture", func(t *testing.T) {
		in := fixtureInput(t)
		for _, file := range []string{"ignored.go", "ignored/ignored.go", "cgo.go", "unlisted.go"} {
			in.Documents[file] = true
		}
		status, rows := Layer(t.Context(), in)
		wantLayer(t, status, rows, nil, []string{"external_test.go", "flow.go", "flow_test.go", "meter/meter.go", "tagged.go", "zz_generated.go"})
		picked := map[contract.Anchor][]string{}
		var previous contract.Anchor
		for i, row := range rows {
			a := row.Anchor
			if i > 0 && (a.File < previous.File || a.File == previous.File &&
				(a.Line < previous.Line || a.Line == previous.Line && a.Col <= previous.Col)) {
				t.Fatalf("rows out of order: %+v then %+v", previous, a)
			}
			picked[a] = row.Attributes
			previous = a
		}
		for _, test := range []struct {
			file string
			line uint32
			col  uint32
			attr string
		}{
			{"flow.go", 8, 5, ""},                    // Count alias
			{"flow.go", 10, 6, ""},                   // DefaultCount
			{"flow.go", 12, 4, ""},                   // A
			{"flow.go", 12, 7, ""},                   // B
			{"flow.go", 14, 7, ""},                   // Marker
			{"flow.go", 17, 5, ""},                   // Shape
			{"flow.go", 18, 1, ""},                   // Shape.Score
			{"flow.go", 38, 5, "//go:noinline"},      // seed
			{"flow.go", 40, 5, ""},                   // Max
			{"flow_test.go", 4, 5, ""},               // TestFlow
			{"external_test.go", 9, 5, ""},           // TestPublicCalls
			{"meter/meter.go", 14, 5, ""},            // Box
			{"meter/meter.go", 20, 16, ""},           // Box.Get
			{"tagged.go", 2, 8, "//go:build go1.21"}, // package flow
			{"zz_generated.go", 5, 5, ""},            // Generated
		} {
			a := contract.Anchor{File: test.file, Line: test.line, Col: test.col}
			attrs, ok := picked[a]
			var want []string
			if test.attr != "" {
				want = []string{test.attr}
			}
			if !ok || !slices.Equal(attrs, want) {
				t.Errorf("anchor %+v: got %v (%v), want %v", a, attrs, ok, want)
			}
		}
		for _, anchor := range []contract.Anchor{
			{File: "flow.go", Line: 14, Col: 4},        // _
			{File: "meter/meter.go", Line: 3, Col: 1},  // Circle.Amount
			{File: "meter/meter.go", Line: 15, Col: 1}, // Box.item
		} {
			if _, ok := picked[anchor]; ok {
				t.Errorf("unexpected row at %+v", anchor)
			}
		}
		data, err := json.Marshal(rows)
		if err != nil {
			t.Fatal(err)
		}
		for _, name := range []string{"visibility", "qualifiers", "body"} {
			if bytes.Contains(data, []byte("\""+name+"\":")) {
				t.Errorf("unexpected field %s in %s", name, data)
			}
		}
	})

	t.Run("blank-identifiers-and-grouped-attributes", func(t *testing.T) {
		const source = "package p\n" +
			"func _() {}\n" +
			"type _ struct{ _ int }\n" +
			"const _ = 1\n" +
			"var _, Keep = 1, 2\n" +
			"type T struct{ Field int }\n" +
			"func (T) _() {}\n" +
			"//go:one\n//go:two\nfunc F() { Local := 1; _ = Local }\n" +
			"var (\n//go:group-item\n G = 1\n H = 0\n)\n"
		root := t.TempDir()
		fset := token.NewFileSet()
		tree := parseTree(t, fset, root, "blank.go", source)
		state := selection{}
		rows := state.items("blank.go", tree, fset, nil)
		want := []contract.SyntaxFact{
			{Anchor: contract.Anchor{File: "blank.go", Line: 4, Col: 7}},
			{Anchor: contract.Anchor{File: "blank.go", Line: 5, Col: 5}},
			{Anchor: contract.Anchor{File: "blank.go", Line: 9, Col: 5}, Attributes: []string{"//go:one", "//go:two"}},
			{Anchor: contract.Anchor{File: "blank.go", Line: 12, Col: 1}, Attributes: []string{"//go:group-item"}},
			{Anchor: contract.Anchor{File: "blank.go", Line: 13, Col: 1}},
		}
		if !reflect.DeepEqual(rows, want) {
			t.Fatalf("rows\n got: %#v\nwant: %#v", rows, want)
		}
	})
}

func fixtureInput(t *testing.T) Input {
	t.Helper()
	root := canonicalDir(t, "../../../../fixtures/go-flow")
	in := Input{Root: root, Env: testEnv(), Files: map[string]File{}, Documents: map[string]bool{}}
	for _, file := range []string{"external_test.go", "flow.go", "flow_test.go", "meter/meter.go", "tagged.go", "zz_generated.go"} {
		in.Files[file] = File{HasLineDirective: file == "zz_generated.go"}
		in.Documents[file] = true
	}
	return in
}

func canonicalDir(t *testing.T, name string) string {
	t.Helper()
	path, err := filepath.Abs(name)
	if err != nil {
		t.Fatal(err)
	}
	root, err := filepath.EvalSymlinks(path)
	if err != nil {
		t.Fatal(err)
	}
	return root
}

func TestLoaderContract(t *testing.T) {
	ctx := t.Context()
	in := Input{Root: t.TempDir(), Env: []string{"GOPACKAGESDRIVER=off", "GOENV=off"}}
	for _, loadErr := range []error{nil, errors.New("load probe")} {
		called := false
		status, rows := loadLayer(ctx, in, func(cfg *packages.Config, patterns ...string) ([]*packages.Package, error) {
			called = true
			const mode = packages.NeedName | packages.NeedFiles | packages.NeedSyntax | packages.NeedTypes |
				packages.NeedTypesInfo | packages.NeedModule
			if cfg.Context != ctx || cfg.Dir != in.Root || !slices.Equal(cfg.Env, in.Env) ||
				!cfg.Tests || cfg.Mode != mode || !slices.Equal(patterns, []string{"./..."}) {
				t.Fatalf("configuration %+v, patterns %v", cfg, patterns)
			}
			return nil, loadErr
		})
		if !called || len(rows) > 0 || !reflect.DeepEqual(status, contract.Failed("load-failed")) {
			t.Fatalf("called=%v, status=%+v, rows=%+v", called, status, rows)
		}
	}
}

func TestVariantSelection(t *testing.T) {
	for _, test := range []struct {
		name      string
		fileCount int
		errorIn   string
		want      string
	}{
		{"most-files", 1, "", "//go:test-variant"},
		{"equal-smaller-id", 2, "", "//go:ordinary"},
		{"error-in-losing-ordinary", 1, "p", ""},
		{"error-in-losing-test", 3, "p [p.test]", ""},
	} {
		t.Run(test.name, func(t *testing.T) {
			root := t.TempDir()
			fset := token.NewFileSet()
			pkg := func(id, directive string, count int) *packages.Package {
				tree := parseTree(t, fset, root, "shared.go", "package p\n"+directive+"\nfunc Shared() {}")
				syntax := []*ast.File{tree}
				for range count - 1 {
					syntax = append(syntax, parseTree(t, fset, root, "other.go", "package p\nvar Extra = 1"))
				}
				p := &packages.Package{ID: id, PkgPath: "p", Fset: fset, Syntax: syntax}
				if test.errorIn == id {
					p.Errors = []packages.Error{{Kind: packages.ListError, Msg: "# p\nshared.go:3:6: unmatched compiler error"}}
				}
				return p
			}
			ordinary := pkg("p", "//go:ordinary", test.fileCount)
			variant := pkg("p [p.test]", "//go:test-variant", 2)
			proof := parseTree(t, fset, root, "independent/ok.go", "package independent\nvar Keep = 1")
			other := &packages.Package{ID: "independent", PkgPath: "independent", Fset: fset, Syntax: []*ast.File{proof}}
			in := Input{Root: root, Files: map[string]File{"shared.go": {}, "independent/ok.go": {}},
				Documents: map[string]bool{"shared.go": true, "independent/ok.go": true}}
			status, rows := loadLayer(t.Context(), in, func(*packages.Config, ...string) ([]*packages.Package, error) {
				return []*packages.Package{variant, other, ordinary}, nil
			})
			if test.errorIn != "" {
				wantLayer(t, status, rows, []contract.Skipped{{File: "shared.go", Code: "package-error"}}, []string{"independent/ok.go"})
				return
			}
			wantLayer(t, status, rows, nil, []string{"independent/ok.go", "shared.go"})
			if len(rows) != 2 || !slices.Equal(rows[1].Attributes, []string{test.want}) {
				t.Fatalf("wanted one shared row from %s, got %v", test.want, rows)
			}
		})
	}
}

func TestAdmission(t *testing.T) {
	root := t.TempDir()
	fset := token.NewFileSet()
	files := map[string]File{"readable.go": {}, "absent-in-index.go": {}, "broken.go": {Code: "parse-failed"}, "../outside.go": {}}
	docs := map[string]bool{"readable.go": true, "broken.go": true, "absent-in-inventory.go": true, "../outside.go": true}
	var trees []*ast.File
	for _, name := range []string{"readable.go", "absent-in-index.go", "broken.go", "absent-in-inventory.go", "../outside.go"} {
		trees = append(trees, parseTree(t, fset, root, name, "package p\nfunc Name() {}"))
	}
	// Two AST items at one location must produce a single fact.
	copy := *trees[0].Decls[0].(*ast.FuncDecl)
	copy.Name = &ast.Ident{Name: "Different", NamePos: copy.Name.Pos()}
	trees[0].Decls = append(trees[0].Decls, &copy)
	var stderr bytes.Buffer
	in := Input{Root: root, Files: files, Documents: docs, Stderr: &stderr}
	status, rows := loadLayer(t.Context(), in, func(*packages.Config, ...string) ([]*packages.Package, error) {
		return []*packages.Package{{ID: "p", PkgPath: "p", Syntax: trees, Fset: fset}}, nil
	})
	wantLayer(t, status, rows, []contract.Skipped{
		{File: "broken.go", Code: "parse-failed"},
		{File: "readable.go", Code: "duplicate-anchor"},
	}, []string{"readable.go"})
	if len(rows) != 1 || !strings.Contains(stderr.String(), "Different at readable.go:2:6") {
		t.Fatalf("rows=%+v, diagnostic=%q", rows, stderr.String())
	}
}

func parseTree(t *testing.T, fset *token.FileSet, root, name, source string) *ast.File {
	t.Helper()
	tree, err := parser.ParseFile(fset, filepath.Join(root, name), source, parser.ParseComments)
	if err != nil {
		t.Fatal(err)
	}
	return tree
}

func wantLayer(t *testing.T, status contract.Entry, rows []contract.SyntaxFact, skipped []contract.Skipped, files []string) {
	t.Helper()
	if status.Status != "ok" || len(status.Diagnostics) > 0 || !slices.Equal(status.Skipped, skipped) {
		t.Fatalf("status %+v, want ok with skips %+v", status, skipped)
	}
	var got []string
	for _, row := range rows {
		got = append(got, row.Anchor.File)
	}
	slices.Sort(got)
	if !slices.Equal(slices.Compact(got), files) {
		t.Fatalf("files %v, want %v", got, files)
	}
}
