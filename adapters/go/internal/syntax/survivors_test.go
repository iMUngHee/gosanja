package syntax

import (
	"errors"
	"go/token"
	"math"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"golang.org/x/tools/go/packages"
	"gosanja/adapters/go/internal/contract"
)

func TestCompilerMatchingKeepsFirstNearestDiagnostic(t *testing.T) {
	diagnosticAt := func(column int) diagnostic {
		return diagnostic{pos: token.Position{Filename: "p.go", Line: 3, Column: column}, text: "a message"}
	}
	typed := []diagnostic{diagnosticAt(8), diagnosticAt(18)}
	for _, column := range []int{9, 13} {
		found := make(matcher, len(typed))
		if !found.take(typed, diagnosticAt(column), true) || !found[0] || found[1] {
			t.Errorf("column %d must use the first candidate, got %v", column, found)
		}
	}
}

func TestDiagnosticPositionRejectsExplicitZero(t *testing.T) {
	root := t.TempDir()
	for _, raw := range []string{"p.go:0", "p.go:3:0"} {
		if got := parsePosition(root, raw); got != (token.Position{}) {
			t.Errorf("parsePosition(%q) = %+v, want no position", raw, got)
		}
	}
}

func TestEmptyOutlineDoesNotNeedDefinitionExtent(t *testing.T) {
	const file = "empty.go"
	in := tempModule(t, map[string]string{file: "package p\nfunc Empty() {}\n"}, "")
	in.Index = outlineIndex(file)
	status, rows := Layer(t.Context(), in)
	wantLayer(t, status, rows, nil, []string{file})
	if len(rows) != 1 || len(rows[0].Outline) != 0 {
		t.Fatalf("want one declaration without an outline, got %+v", rows)
	}
}

func TestGroupedTypeKeepsItsOwnDirective(t *testing.T) {
	const file = "p.go"
	const source = "package p\ntype (\n//go:type-item\n Item struct{}\n Plain struct{}\n)\n"
	fset := token.NewFileSet()
	tree := parseTree(t, fset, t.TempDir(), file, source)
	state := selection{}
	rows := state.items(file, tree, fset, nil)
	if len(rows) != 2 || len(rows[0].Attributes) != 1 || rows[0].Attributes[0] != "//go:type-item" ||
		len(rows[1].Attributes) != 0 {
		t.Fatalf("want the directive on the first type only, got %+v", rows)
	}
}

func TestZeroBasedPositionsAcceptRepresentableBounds(t *testing.T) {
	if line, col, ok := zeroBased(token.Position{Line: 1, Column: 1}); line != 0 || col != 0 || !ok {
		t.Errorf("position 1:1 yielded %d:%d, ok=%v", line, col, ok)
	}
	if strconv.IntSize < 64 {
		return
	}
	var maximum uint32 = math.MaxUint32
	last := int(maximum) + 1
	if line, col, ok := zeroBased(token.Position{Line: last, Column: last}); line != maximum || col != maximum || !ok {
		t.Errorf("position %d:%d yielded %d:%d, ok=%v", last, last, line, col, ok)
	}
}

func TestIfWithElseKeepsInitializerCall(t *testing.T) {
	const file = "conditional.go"
	const declaration = "func F(fn func() int) { if value := fn(); value > 0 {} else {} }"
	in := tempModule(t, map[string]string{file: "package p\n" + declaration + "\n"}, "")
	in.Index = outlineIndex(file,
		testDefinition("F", []int32{1, 5, 6}, []int32{1, 0, 1, int32(len(declaration))}),
	)
	status, rows := Layer(t.Context(), in)
	wantLayer(t, status, rows, nil, []string{file})
	outline := outlineAt(t, rows, file, 1, 5)
	if len(outline) != 1 || outline[0].K != "branch" || len(outline[0].Children) != 2 ||
		outline[0].Children[0].K != "call" || outline[0].Children[0].Callee == nil ||
		outline[0].Children[1].K != "arm" {
		t.Fatalf("wanted initializer call and else arm, got:\n%s", renderOutline(outline))
	}
}

func TestElseArmFitsAtMaximumOutlineDepth(t *testing.T) {
	const file = "nested.go"
	declaration := "func F() { " + strings.Repeat("if true { ", 30) +
		"if true {} else {}" + strings.Repeat(" }", 30) + " }"
	in := tempModule(t, map[string]string{file: "package p\n" + declaration + "\n"}, "")
	in.Index = outlineIndex(file,
		testDefinition("F", []int32{1, 5, 6}, []int32{1, 0, 1, int32(len(declaration))}),
	)
	status, rows := Layer(t.Context(), in)
	wantLayer(t, status, rows, nil, []string{file})
	if len(outlineAt(t, rows, file, 1, 5)) != 1 {
		t.Fatalf("expected an outline for the deepest permitted else arm, got %+v", rows)
	}
}

func TestElseChildCountsTowardOutlineDepth(t *testing.T) {
	const file = "nested.go"
	declaration := "func F() { " + strings.Repeat("if true { ", 30) +
		"if true {} else { return }" + strings.Repeat(" }", 30) + " }"
	in := tempModule(t, map[string]string{file: "package p\n" + declaration + "\n"}, "")
	in.Index = outlineIndex(file,
		testDefinition("F", []int32{1, 5, 6}, []int32{1, 0, 1, int32(len(declaration))}),
	)
	status, rows := Layer(t.Context(), in)
	if status.Status != "ok" || len(status.Skipped) != 1 || status.Skipped[0].Code != "outline-too-deep" ||
		len(rows) != 1 || len(rows[0].Outline) != 0 {
		t.Fatalf("wanted the declaration and an over-depth outline diagnostic, got %+v, %+v", status, rows)
	}
}

func TestLoadFailureReportsTheToolError(t *testing.T) {
	var stderr strings.Builder
	status, rows := loadLayer(t.Context(), Input{Stderr: &stderr}, func(*packages.Config, ...string) ([]*packages.Package, error) {
		return nil, errors.New("loader process failed")
	})
	if status.Status != "failed" || len(rows) != 0 || stderr.String() != "load Go syntax: loader process failed\n" {
		t.Fatalf("status = %+v, rows = %v, stderr = %q", status, rows, stderr.String())
	}
}

func TestFailedLoaderAccountsForListedFileWithoutAST(t *testing.T) {
	root := t.TempDir()
	const file = "listed.go"
	in := Input{Root: root, Files: map[string]File{file: {}}, Documents: map[string]bool{file: true}}
	status, rows := loadLayer(t.Context(), in, func(*packages.Config, ...string) ([]*packages.Package, error) {
		return []*packages.Package{{
			ID: "p", PkgPath: "p", GoFiles: []string{filepath.Join(root, file)},
			Errors: []packages.Error{{Kind: packages.ListError, Msg: "failed to parse package"}},
		}}, nil
	})
	wantLayer(t, status, rows, []contract.Skipped{{File: file, Code: "package-error"}}, nil)
}

func TestPortablePathRejectsDriveLetters(t *testing.T) {
	for name, want := range map[string]bool{
		"a": true, "pkg/file.go": true, "0:/file.go": true,
		"A:/file.go": false, "Z:/file.go": false,
		"a:/file.go": false, "z:/file.go": false,
	} {
		if got := portable(name); got != want {
			t.Errorf("portable(%q) = %v, want %v", name, got, want)
		}
	}
}
