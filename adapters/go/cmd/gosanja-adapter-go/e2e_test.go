package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"slices"
	"sort"
	"strings"
	"testing"

	"github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"
)

// The engine and the adapter, built once per test binary. A test that needs them fails, and
// is never skipped, when a build failed.
var (
	engineBin, adapterDir string
	buildErr              error
)

func TestMain(m *testing.M) {
	scratch, err := os.MkdirTemp("", "gosanja-adapter-go-e2e-")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	engineBin, adapterDir, buildErr = build(scratch)
	code := m.Run()
	_ = os.RemoveAll(scratch)
	os.Exit(code)
}

// build builds gosanja with cargo and the adapter with go into scratch.
func build(scratch string) (engine, adapters string, err error) {
	repoRoot, err := filepath.Abs("../../../..")
	if err != nil {
		return "", "", err
	}
	cargo := exec.Command("cargo", "build", "--locked", "-p", "gosanja", "--bin", "gosanja", "--message-format=json")
	cargo.Dir = repoRoot
	cargo.Stderr = os.Stderr
	out, err := cargo.Output()
	if err != nil {
		return "", "", fmt.Errorf("cargo build: %w", err)
	}
	for _, line := range bytes.Split(out, []byte("\n")) {
		var msg struct {
			Reason     string                `json:"reason"`
			Executable string                `json:"executable"`
			Target     struct{ Name string } `json:"target"`
		}
		if json.Unmarshal(line, &msg) == nil && msg.Reason == "compiler-artifact" && msg.Target.Name == "gosanja" && msg.Executable != "" {
			engine = msg.Executable
		}
	}
	if engine == "" {
		return "", "", fmt.Errorf("cargo build named no gosanja executable")
	}
	adapters = filepath.Join(scratch, "adapters")
	gobuild := exec.Command("go", "build", "-o", filepath.Join(adapters, "gosanja-adapter-go"), ".")
	gobuild.Env = append(os.Environ(), "CGO_ENABLED=0")
	if out, err := gobuild.CombinedOutput(); err != nil {
		return "", "", fmt.Errorf("go build: %v\n%s", err, out)
	}
	return engine, adapters, nil
}

// engine runs gosanja with an environment of the world's alone (its tools first on PATH, its
// home, an offline proxy, the test's module cache and the built adapter) and gives its
// exit status and its standard output and error together.
func (w *world) engine(args ...string) (int, string) {
	w.t.Helper()
	return w.engineWith("", args...)
}

// engineWith is engine with another module cache, when cache is not empty.
func (w *world) engineWith(cache string, args ...string) (int, string) {
	w.t.Helper()
	if buildErr != nil {
		w.t.Fatalf("the engine or the adapter did not build: %v", buildErr)
	}
	if cache == "" {
		cache = moduleCache(w.t)
	}
	cmd := exec.Command(engineBin, args...)
	cmd.Dir = w.dir
	cmd.Env = []string{"PATH=" + w.bin + ":/usr/bin:/bin", "HOME=" + w.home, "GOPROXY=off",
		"GOMODCACHE=" + cache, "GOSANJA_ADAPTER_PATH=" + adapterDir}
	var output bytes.Buffer
	cmd.Stdout, cmd.Stderr = &output, &output
	err := cmd.Run()
	if exit, ok := err.(*exec.ExitError); ok {
		return exit.ExitCode(), output.String()
	}
	if err != nil {
		w.t.Fatal(err)
	}
	return 0, output.String()
}

// snapshot is what a test reads of a published snapshot.
type snapshot struct {
	layers map[string]string
	gaps   []map[string]any
	rows   map[string]bool // files of the entities that have a syntax row
}

func readSnapshot(t *testing.T, dir string) snapshot {
	t.Helper()
	var manifest struct {
		Scopes []struct{ Layers map[string]string } `json:"scopes"`
	}
	data, err := os.ReadFile(filepath.Join(dir, "manifest.json"))
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(data, &manifest); err != nil || len(manifest.Scopes) != 1 {
		t.Fatalf("manifest %s: %v", data, err)
	}
	s := snapshot{layers: manifest.Scopes[0].Layers, rows: map[string]bool{}}
	for _, g := range jsonl(t, filepath.Join(dir, "gaps.jsonl")) {
		s.gaps = append(s.gaps, g)
	}
	fileOf := map[string]string{}
	for _, e := range jsonl(t, filepath.Join(dir, "entities.jsonl")) {
		if def, ok := e["def"].(map[string]any); ok {
			fileOf[e["key"].(string)] = def["file"].(string)
		}
	}
	for _, r := range jsonl(t, filepath.Join(dir, "syntax.jsonl")) {
		s.rows[fileOf[r["key"].(string)]] = true
	}
	return s
}

func jsonl(t *testing.T, file string) []map[string]any {
	t.Helper()
	f, err := os.Open(file)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	var rows []map[string]any
	scanner := bufio.NewScanner(f)
	scanner.Buffer(nil, 1<<24)
	for scanner.Scan() {
		var row map[string]any
		if err := json.Unmarshal(scanner.Bytes(), &row); err != nil {
			t.Fatalf("%s: %v", file, err)
		}
		rows = append(rows, row)
	}
	return rows
}

// skippedGaps are the snapshot's skipped-file gaps as "layer file code".
func (s snapshot) skippedGaps() []string {
	var out []string
	for _, g := range s.gaps {
		if g["gap"] == "skipped-file" {
			out = append(out, fmt.Sprintf("%s %s %s", g["layer"], g["file"], g["code"]))
		}
	}
	sort.Strings(out)
	return out
}

// synthIndex writes root/index.scip as scip-go would for the module's files that parse, except
// those in leave: one document per file, a definition for each package (in its first file),
// function, method, type and package-level variable or constant, with the declaration as the
// enclosing range of functions, methods and types. narrow names functions
// whose enclosing range is cut to their first line.
func synthIndex(t *testing.T, root, module string, leave []string, narrow ...string) {
	t.Helper()
	var files []string
	err := filepath.WalkDir(root, func(p string, d os.DirEntry, err error) error {
		if err == nil && !d.IsDir() && strings.HasSuffix(p, ".go") {
			rel, _ := filepath.Rel(root, p)
			files = append(files, filepath.ToSlash(rel))
		}
		return err
	})
	if err != nil {
		t.Fatal(err)
	}
	sort.Strings(files)
	idx := &scip.Index{Metadata: &scip.Metadata{ToolInfo: &scip.ToolInfo{Name: "scip-go", Version: "0.2.10"},
		ProjectRoot: "file:///synthetic", TextDocumentEncoding: scip.TextEncoding_UTF8}}
	packaged := map[string]bool{}
	for _, rel := range files {
		if slices.Contains(leave, rel) {
			continue
		}
		fset := token.NewFileSet()
		f, err := parser.ParseFile(fset, filepath.Join(root, rel), nil, parser.SkipObjectResolution)
		if err != nil {
			continue
		}
		dir := path.Dir(rel)
		pkg := module
		if dir != "." {
			pkg = module + "/" + dir
		}
		prefix := "scip-go gomod " + module + " v0.0.0 `" + pkg + "`/"
		doc := &scip.Document{RelativePath: rel, Language: "go", PositionEncoding: scip.PositionEncoding_UTF8CodeUnitOffsetFromLineStart}
		define := func(symbol string, kind scip.SymbolInformation_Kind, name *ast.Ident, decl ast.Node) {
			start, end := fset.PositionFor(name.Pos(), false), fset.PositionFor(name.End(), false)
			occ := &scip.Occurrence{Symbol: symbol, SymbolRoles: int32(scip.SymbolRole_Definition),
				Range: []int32{int32(start.Line - 1), int32(start.Column - 1), int32(end.Column - 1)}}
			if decl != nil {
				from, to := fset.PositionFor(decl.Pos(), false), fset.PositionFor(decl.End(), false)
				occ.EnclosingRange = []int32{int32(from.Line - 1), int32(from.Column - 1), int32(to.Line - 1), int32(to.Column - 1)}
				for _, n := range narrow {
					if n == name.Name {
						occ.EnclosingRange = []int32{int32(from.Line - 1), int32(from.Column - 1), int32(from.Line - 1), int32(from.Column)}
					}
				}
			}
			doc.Occurrences = append(doc.Occurrences, occ)
			doc.Symbols = append(doc.Symbols, &scip.SymbolInformation{Symbol: symbol, Kind: kind, DisplayName: name.Name})
		}
		if !packaged[pkg] {
			packaged[pkg] = true
			define(prefix, scip.SymbolInformation_Package, f.Name, nil)
		}
		for _, decl := range f.Decls {
			switch d := decl.(type) {
			case *ast.FuncDecl:
				symbol := prefix + d.Name.Name + "()."
				if d.Recv != nil && len(d.Recv.List) == 1 {
					typ := d.Recv.List[0].Type
					if star, ok := typ.(*ast.StarExpr); ok {
						typ = star.X
					}
					if id, ok := typ.(*ast.Ident); ok {
						symbol = prefix + id.Name + "#" + d.Name.Name + "()."
					}
				}
				define(symbol, scip.SymbolInformation_Function, d.Name, d)
			case *ast.GenDecl:
				for _, spec := range d.Specs {
					switch sp := spec.(type) {
					case *ast.TypeSpec:
						define(prefix+sp.Name.Name+"#", scip.SymbolInformation_Type, sp.Name, sp)
					case *ast.ValueSpec:
						for _, name := range sp.Names {
							define(prefix+name.Name+".", scip.SymbolInformation_Variable, name, nil)
						}
					}
				}
			}
		}
		idx.Documents = append(idx.Documents, doc)
	}
	data, err := proto.Marshal(idx)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "index.scip"), data, 0o644); err != nil {
		t.Fatal(err)
	}
}
