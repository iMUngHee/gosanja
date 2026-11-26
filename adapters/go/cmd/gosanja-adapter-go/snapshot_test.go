package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"sort"
	"strings"
	"testing"

	"github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"
	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/scipindex"
)

// flowSnapshot indexes a copy of go-flow, named go-flow, through the engine, which finds the
// adapter by itself, and validates the snapshot.
func flowSnapshot(t *testing.T, w *world, extra ...string) (root, out string) {
	t.Helper()
	root = w.flow("go-flow")
	out = filepath.Join(w.dir, "snapshot-"+strings.Join(extra, "-"))
	args := append([]string{"index", "--root", root, "--out", out}, extra...)
	if code, stderr := w.engine(args...); code != 0 {
		t.Fatalf("gosanja index exit %d: %s", code, stderr)
	}
	if code, stderr := w.engine("validate", out, "--root", root); code != 0 {
		t.Fatalf("gosanja validate exit %d: %s", code, stderr)
	}
	return root, out
}

// sameTree reports the files that differ between two directories, byte for byte.
func sameTree(t *testing.T, got, want string) []string {
	t.Helper()
	read := func(dir string) map[string][]byte {
		files := map[string][]byte{}
		err := filepath.WalkDir(dir, func(p string, d os.DirEntry, err error) error {
			if err != nil || d.IsDir() {
				return err
			}
			rel, _ := filepath.Rel(dir, p)
			files[rel], err = os.ReadFile(p)
			return err
		})
		if err != nil {
			t.Fatal(err)
		}
		return files
	}
	g, w := read(got), read(want)
	var differ []string
	for name := range g {
		if !bytes.Equal(g[name], w[name]) {
			differ = append(differ, name)
		}
	}
	for name := range w {
		if _, ok := g[name]; !ok {
			differ = append(differ, name)
		}
	}
	sort.Strings(differ)
	return differ
}

func TestEndToEnd(t *testing.T) {
	w := newWorld(t)
	_, out := flowSnapshot(t, w)
	if differ := sameTree(t, out, filepath.Join(repo(t), "fixtures", "go-flow", "expected")); len(differ) != 0 {
		t.Errorf("the snapshot differs from fixtures/go-flow/expected in %v", differ)
	}
	var manifest struct {
		Scopes []struct {
			Adapter struct{ Name, Contract string } `json:"adapter"`
			Layers  map[string]string               `json:"layers"`
		} `json:"scopes"`
	}
	data, _ := os.ReadFile(filepath.Join(out, "manifest.json"))
	if err := json.Unmarshal(data, &manifest); err != nil || len(manifest.Scopes) != 1 {
		t.Fatalf("manifest %s: %v", data, err)
	}
	scope := manifest.Scopes[0]
	if scope.Adapter.Name != "go" || scope.Adapter.Contract != "0.5" ||
		!reflect.DeepEqual(scope.Layers, map[string]string{"base": "present", "syntax": "present", "calls": "absent", "build": "present"}) {
		t.Errorf("scope %+v", scope)
	}
	if n := countGaps(readSnapshot(t, out), "missing-layer", "calls"); n != 1 {
		t.Errorf("%d missing-layer gaps for calls", n)
	}

	_, requested := flowSnapshot(t, w, "--layers", "base,syntax")
	s := readSnapshot(t, requested)
	if s.layers["calls"] != "not-requested" || countGaps(s, "missing-layer", "calls") != 0 {
		t.Errorf("--layers base,syntax: layers %v, gaps %v", s.layers, s.gaps)
	}

	code, stderr := w.engine("adapter", "list")
	if code != 0 {
		t.Fatalf("gosanja adapter list exit %d", code)
	}
	listed := false
	for _, line := range strings.Split(stderr, "\n") {
		listed = listed || strings.HasPrefix(line, "go ")
	}
	if !listed || !strings.Contains(stderr, "requires go: found") || !strings.Contains(stderr, "requires scip-go: found") {
		t.Errorf("gosanja adapter list:\n%s", stderr)
	}
}

func countGaps(s snapshot, kind, layer string) int {
	n := 0
	for _, g := range s.gaps {
		if g["gap"] == kind && (layer == "" || g["layer"] == layer) {
			n++
		}
	}
	return n
}

const flowKey = "gomod:example.com/goflow:`example.com/goflow`/"
const meterKey = "gomod:example.com/goflow:`example.com/goflow/meter`/"
const testKey = "gomod:example.com/goflow:`example.com/goflow_test`/"

func TestMergedPackage(t *testing.T) {
	const module = "example.com/merge"
	const key = "gomod:" + module + ":`" + module + "`/"
	const constraint = "//go:build go1.21"
	for _, tc := range []struct {
		name, second string
		kept         []string
	}{
		{"one_kept_one_excluded", "", []string{"a.go"}},
		{"two_kept_one_excluded", "package m\n\nfunc B() {}\n", []string{"a.go", "b.go"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			w := newWorld(t)
			root := w.simple("merge", map[string]string{
				"a.go":        constraint + "\n\npackage m\n\nfunc A() {}\n",
				"b.go":        tc.second,
				"excluded.go": "//go:build gosanja_never\n\npackage m\n",
			})
			synthIndex(t, root, module, nil)
			produced, err := scipindex.Read(filepath.Join(root, "index.scip"))
			if err != nil {
				t.Fatal(err)
			}
			if want := append(slices.Clone(tc.kept), "excluded.go"); !slices.Equal(produced.Paths(), want) {
				t.Fatalf("produced paths %v, want %v", produced.Paths(), want)
			}
			baseOut, code, stderr := w.index(w.caller(nil), root, "--layers", "base")
			if code != 0 {
				t.Fatalf("adapter index exit %d: %s", code, stderr)
			}
			retained, err := scipindex.Read(filepath.Join(baseOut, "index.scip"))
			if err != nil {
				t.Fatal(err)
			}
			if !slices.Equal(retained.Paths(), tc.kept) {
				t.Fatalf("published paths %v, want %v", retained.Paths(), tc.kept)
			}
			var wantMerged []string
			if len(tc.kept) == 2 {
				wantMerged = []string{"scip-go gomod " + module + " v0.0.0 `" + module + "`/"}
			}
			if got := result(t, baseOut).Layers[contract.Base].Merged; !slices.Equal(got, wantMerged) {
				t.Errorf("base merged = %v, want %v", got, wantMerged)
			}
			out := filepath.Join(w.dir, "snapshot")
			if code, stderr := w.engine("index", "--root", root, "--out", out); code != 0 {
				t.Fatalf("gosanja index exit %d: %s", code, stderr)
			}
			if code, stderr := w.engine("validate", out, "--root", root); code != 0 {
				t.Fatalf("gosanja validate exit %d: %s", code, stderr)
			}
			if len(tc.kept) == 2 {
				assertMergedPackage(t, out, key, tc.kept)
				return
			}
			var def, tag bool
			for _, row := range jsonl(t, filepath.Join(out, "entities.jsonl")) {
				if row["key"] == key {
					site, _ := row["def"].(map[string]any)
					def = site["file"] == "a.go" && row["decls"] == nil
				}
			}
			for _, row := range jsonl(t, filepath.Join(out, "files.jsonl")) {
				if row["path"] == "a.go" {
					tag = row["constraint"] == constraint
				}
			}
			if !def || !tag {
				t.Errorf("%s: def in a.go = %t, file row with build constraint = %t", key, def, tag)
			}
		})
	}
}

func TestConflictingPackageNamesStayCollision(t *testing.T) {
	const module = "example.com/conflict"
	const key = "gomod:" + module + ":`" + module + "`/"
	w := newWorld(t)
	root := w.simple("conflict", map[string]string{
		"a.go": "package p\n",
		"b.go": "package p\n",
		"c.go": "package q\n",
	})
	// The index gives all three package clauses the same namespace symbol.
	synthIndex(t, root, module, nil)
	baseOut, code, stderr := w.index(w.caller(nil), root, "--layers", "base")
	if code != 0 {
		t.Fatalf("adapter index exit %d: %s", code, stderr)
	}
	if got := result(t, baseOut).Layers[contract.Base].Merged; len(got) != 0 {
		t.Errorf("base merged = %v, want no symbols for conflicting package names", got)
	}
	out := filepath.Join(w.dir, "snapshot")
	if code, stderr := w.engine("index", "--root", root, "--out", out, "--layers", "base"); code != 0 {
		t.Fatalf("gosanja index exit %d: %s", code, stderr)
	}
	assertPackageCollision(t, out, key, []string{"a.go", "b.go", "c.go"})
}

func TestUnverifiedPackageDefinitionStaysCollision(t *testing.T) {
	const module = "example.com/unverified"
	const key = "gomod:" + module + ":`" + module + "`/"
	for _, name := range []string{"utf16", "invalid_utf8", "outside_clause"} {
		t.Run(name, func(t *testing.T) {
			w := newWorld(t)
			root := w.simple("unverified", map[string]string{
				"a.go": "package p\n",
				"b.go": "package p\n",
				"c.go": "package q\n",
			})
			synthIndex(t, root, module, nil)
			if name == "invalid_utf8" {
				w.write(filepath.Join(root, "c.go"), "package q\n// \xff\n", 0o644)
			} else {
				indexFile := filepath.Join(root, "index.scip")
				data, err := os.ReadFile(indexFile)
				if err != nil {
					t.Fatal(err)
				}
				var index scip.Index
				if err := proto.Unmarshal(data, &index); err != nil {
					t.Fatal(err)
				}
				for _, doc := range index.Documents {
					if doc.RelativePath == "c.go" {
						if name == "utf16" {
							doc.PositionEncoding = scip.PositionEncoding_UTF16CodeUnitOffsetFromLineStart
						} else {
							doc.Occurrences[0].TypedRange = &scip.Occurrence_SingleLineRange{
								SingleLineRange: &scip.SingleLineRange{Line: 0, StartCharacter: 0, EndCharacter: 7},
							}
						}
					}
				}
				if err := os.WriteFile(indexFile, marshal(t, &index), 0o644); err != nil {
					t.Fatal(err)
				}
			}
			baseOut, code, stderr := w.index(w.caller(nil), root, "--layers", "base")
			if code != 0 {
				t.Fatalf("adapter index exit %d: %s", code, stderr)
			}
			retained, err := scipindex.Read(filepath.Join(baseOut, "index.scip"))
			if err != nil {
				t.Fatal(err)
			}
			want := []string{"a.go", "b.go", "c.go"}
			if !slices.Equal(retained.Paths(), want) {
				t.Fatalf("published paths = %v, want %v", retained.Paths(), want)
			}
			if got := result(t, baseOut).Layers[contract.Base].Merged; len(got) != 0 {
				t.Errorf("base merged = %v, want no symbol with an unverified definition", got)
			}
			out := filepath.Join(w.dir, "snapshot")
			if code, stderr := w.engine("index", "--root", root, "--out", out, "--layers", "base"); code != 0 {
				t.Fatalf("gosanja index exit %d: %s", code, stderr)
			}
			assertPackageCollision(t, out, key, want)
		})
	}
}

func assertPackageCollision(t *testing.T, out, key string, want []string) {
	t.Helper()
	var group bool
	var members []string
	for _, row := range jsonl(t, filepath.Join(out, "entities.jsonl")) {
		if row["key"] == key {
			group = row["group"] == true
		}
		if row["member_of"] == key {
			def, _ := row["def"].(map[string]any)
			members = append(members, fmt.Sprint(def["file"]))
		}
	}
	if !group || !slices.Equal(members, want) {
		t.Errorf("%s: group = %t, member files = %v, want %v", key, group, members, want)
	}
	if got := countGaps(readSnapshot(t, out), "symbol-collision", ""); got != 1 {
		t.Errorf("got %d symbol-collision gaps, want 1", got)
	}
}

func TestPackageClauseSurvivesLaterParseError(t *testing.T) {
	const module = "example.com/recovered"
	const key = "gomod:" + module + ":`" + module + "`/"
	w := newWorld(t)
	root := w.simple("recovered", map[string]string{
		"a.go": "package p\n\nfunc A() {}\n",
		"b.go": "package p\n",
	})
	// The indexer recovered the package clause. There is no SCIP body in b.go.
	synthIndex(t, root, module, nil)
	w.write(filepath.Join(root, "b.go"), "package p\n@\n", 0o644)
	baseOut, code, stderr := w.index(w.caller(nil), root, "--layers", "base")
	if code != 0 {
		t.Fatalf("adapter index exit %d: %s", code, stderr)
	}
	base := result(t, baseOut).Layers[contract.Base]
	want := []string{"scip-go gomod " + module + " v0.0.0 `" + module + "`/"}
	if !slices.Equal(base.Merged, want) {
		t.Errorf("base merged = %v, want %v", base.Merged, want)
	}
	if !slices.Contains(base.Skipped, contract.Skipped{File: "b.go", Code: "parse-failed"}) {
		t.Errorf("base skipped = %v, want b.go parse-failed", base.Skipped)
	}
	out := filepath.Join(w.dir, "snapshot")
	if code, stderr := w.engine("index", "--root", root, "--out", out); code != 0 {
		t.Fatalf("gosanja index exit %d: %s", code, stderr)
	}
	if code, stderr := w.engine("validate", out, "--root", root); code != 0 {
		t.Fatalf("gosanja validate exit %d: %s", code, stderr)
	}
	assertMergedPackage(t, out, key, []string{"a.go", "b.go"})
	if got := readSnapshot(t, out).skippedGaps(); !slices.Contains(got, "syntax b.go parse-failed") {
		t.Errorf("skipped-file gaps %v, want syntax b.go parse-failed", got)
	}
}

func TestInitStaysCollision(t *testing.T) {
	const module = "example.com/inits"
	const key = "gomod:" + module + ":`" + module + "`/"
	const initKey = key + "init()."
	w := newWorld(t)
	root := w.simple("inits", map[string]string{
		"a.go": "package m\n\nfunc init() {}\n",
		"b.go": "package m\n\nfunc init() {}\n",
	})
	synthIndex(t, root, module, nil)
	baseOut, code, stderr := w.index(w.caller(nil), root, "--layers", "base")
	if code != 0 {
		t.Fatalf("adapter index exit %d: %s", code, stderr)
	}
	want := []string{"scip-go gomod " + module + " v0.0.0 `" + module + "`/"}
	if got := result(t, baseOut).Layers[contract.Base].Merged; !slices.Equal(got, want) {
		t.Errorf("base merged = %v, want only the package symbol %v", got, want)
	}
	out := filepath.Join(w.dir, "snapshot")
	if code, stderr := w.engine("index", "--root", root, "--out", out); code != 0 {
		t.Fatalf("gosanja index exit %d: %s", code, stderr)
	}
	assertMergedPackage(t, out, key, []string{"a.go", "b.go"})
	var group bool
	var members []string
	for _, row := range jsonl(t, filepath.Join(out, "entities.jsonl")) {
		if row["key"] == initKey {
			group = row["group"] == true
		}
		if row["member_of"] == initKey {
			def, _ := row["def"].(map[string]any)
			members = append(members, fmt.Sprint(def["file"]))
		}
	}
	if !group || !slices.Equal(members, []string{"a.go", "b.go"}) {
		t.Errorf("%s: group = %t, member files = %v", initKey, group, members)
	}
	var gaps []map[string]any
	for _, gap := range readSnapshot(t, out).gaps {
		if gap["gap"] == "symbol-collision" {
			gaps = append(gaps, gap)
		}
	}
	if len(gaps) != 1 || !reflect.DeepEqual(gaps[0]["keys"], []any{initKey, initKey + "~1", initKey + "~2"}) {
		t.Errorf("symbol-collision gaps %v, want one for %s and its two members", gaps, initKey)
	}
}

func assertMergedPackage(t *testing.T, out, key string, want []string) {
	t.Helper()
	var matches int
	for _, row := range jsonl(t, filepath.Join(out, "entities.jsonl")) {
		name := row["key"].(string)
		if strings.HasPrefix(name, key+"~") {
			t.Errorf("unexpected package collision member %s", name)
		}
		if name != key {
			continue
		}
		matches++
		var files []string
		decls, _ := row["decls"].([]any)
		for _, decl := range decls {
			files = append(files, decl.(map[string]any)["file"].(string))
		}
		if !slices.Equal(files, want) || row["def"] != nil || row["group"] != nil {
			t.Errorf("%s: expected package declarations at %v, got %v", key, want, row)
		}
	}
	if matches != 1 {
		t.Errorf("found %d rows for package %s", matches, key)
	}
	for _, row := range jsonl(t, filepath.Join(out, "syntax.jsonl")) {
		name := row["key"].(string)
		if name == key || strings.HasPrefix(name, key+"~") {
			t.Errorf("unexpected package-clause syntax row %v", row)
		}
	}
	for _, gap := range readSnapshot(t, out).gaps {
		if keys, _ := gap["keys"].([]any); gap["gap"] == "symbol-collision" && slices.Contains(keys, any(key)) {
			t.Errorf("unexpected package collision %v", gap)
		}
	}
}

func TestSnapshotFacts(t *testing.T) {
	w := newWorld(t)
	_, out := flowSnapshot(t, w)
	_, control := flowSnapshot(t, w, "--layers", "base")
	s := readSnapshot(t, out)
	for _, dir := range []string{out, control} {
		assertMergedPackage(t, dir, flowKey, []string{"flow.go", "flow_test.go", "tagged.go", "zz_generated.go"})
		var meter map[string]any
		for _, row := range jsonl(t, filepath.Join(dir, "entities.jsonl")) {
			if row["key"] == meterKey {
				meter = row
			}
		}
		if def, _ := meter["def"].(map[string]any); def["file"] != "meter/meter.go" {
			t.Errorf("%s: single-file meter package missing def: %v", dir, meter)
		}
	}
	if len(s.gaps) != 12 {
		t.Errorf("got %d gaps, want 12", len(s.gaps))
	}

	// Each item, at the name position read off the source (zero-based line, byte column), has
	// a syntax row keyed to the entity defined there.
	entities := jsonl(t, filepath.Join(out, "entities.jsonl"))
	rows := map[string]map[string]any{}
	for _, r := range jsonl(t, filepath.Join(out, "syntax.jsonl")) {
		rows[r["key"].(string)] = r
	}
	at := func(file string, line, col int) (string, bool) {
		for _, e := range entities {
			def, _ := e["def"].(map[string]any)
			name, _ := def["name"].([]any)
			if def["file"] == file && len(name) == 4 && name[0] == float64(line) && name[1] == float64(col) {
				return e["key"].(string), true
			}
		}
		return "", false
	}
	for _, item := range []struct {
		file      string
		line, col int
		key       string
	}{
		{"flow.go", 8, 5, flowKey + "Count#"},
		{"flow.go", 10, 6, flowKey + "DefaultCount."},
		{"flow.go", 12, 4, flowKey + "A."},
		{"flow.go", 12, 7, flowKey + "B."},
		{"flow.go", 13, 4, flowKey + "F."},
		{"flow.go", 13, 7, flowKey + "G."},
		{"flow.go", 14, 7, flowKey + "Marker."},
		{"flow.go", 17, 5, flowKey + "Shape#"},
		{"flow.go", 18, 1, flowKey + "Shape#Score."},
		{"flow.go", 22, 5, flowKey + "Loader#"},
		{"flow.go", 23, 1, flowKey + "Loader#Load."},
		{"flow.go", 29, 5, flowKey + "EmbeddedValue#"},
		{"flow.go", 33, 5, flowKey + "EmbeddedPointer#"},
		{"flow.go", 38, 5, flowKey + "seed()."},
		{"flow.go", 40, 5, flowKey + "Max()."},
		{"flow.go", 42, 5, flowKey + "make2()."},
		{"flow.go", 46, 5, flowKey + "IsVersion()."},
		{"flow.go", 49, 5, flowKey + "Promoted()."},
		{"flow.go", 56, 5, flowKey + "signal()."},
		{"flow.go", 57, 5, flowKey + "awaitSignal()."},
		{"flow.go", 60, 5, flowKey + "Flow()."},
		{"meter/meter.go", 2, 5, meterKey + "Circle#"},
		{"meter/meter.go", 6, 16, meterKey + "Circle#Score()."},
		{"meter/meter.go", 8, 5, meterKey + "Gauge#"},
		{"meter/meter.go", 12, 16, meterKey + "Gauge#Load()."},
		{"meter/meter.go", 14, 5, meterKey + "Box#"},
		{"meter/meter.go", 18, 5, meterKey + "NewBox()."},
		{"meter/meter.go", 20, 16, meterKey + "Box#Get()."},
		{"meter/meter.go", 22, 5, meterKey + "Double()."},
		{"meter/meter.go", 24, 5, meterKey + "Max()."},
		{"tagged.go", 4, 5, flowKey + "Tagged()."},
		{"external_test.go", 9, 5, testKey + "TestPublicCalls()."},
	} {
		key, ok := at(item.file, item.line, item.col)
		switch {
		case !ok:
			t.Errorf("no entity defined at %s %d:%d", item.file, item.line, item.col)
		case key != item.key:
			t.Errorf("%s %d:%d: entity %s, want %s", item.file, item.line, item.col, key, item.key)
		case rows[key] == nil:
			t.Errorf("no syntax row for %s", key)
		}
	}
	for _, g := range s.gaps {
		if g["layer"] == "syntax" && (g["gap"] == "unanchored" || g["gap"] == "position-out-of-range" || g["gap"] == "failed-layer") {
			t.Errorf("gap %v", g)
		}
	}

	// Declared calls, each by its evidence site, none of them in the control.
	calls := callSites(t, out)
	controlCalls := callSites(t, control)
	for _, want := range []struct {
		what, from, to string
		line, col      int
	}{
		{"a static call after a non-ASCII identifier", flowKey + "Flow().", flowKey + "seed().", 65, 19},
		{"a method call", flowKey + "Promoted().", meterKey + "Circle#Score().", 53, 15},
		{"a promoted call through an embedded value", flowKey + "Promoted().", meterKey + "Circle#Score().", 53, 31},
		{"a promoted call through an embedded pointer", flowKey + "Promoted().", meterKey + "Gauge#Load().", 53, 49},
		{"an instantiated generic call in the package", flowKey + "Flow().", flowKey + "Max().", 63, 32},
		{"an instantiated generic call across packages", flowKey + "Flow().", meterKey + "Max().", 63, 54},
		{"an inferred generic call across packages", flowKey + "Flow().", meterKey + "Max().", 63, 76},
		{"a call from the external test", testKey + "TestPublicCalls().", flowKey + "Max().", 10, 16},
	} {
		site := fmt.Sprintf("%s -> %s at %d:%d", want.from, want.to, want.line, want.col)
		if !calls[site] {
			t.Errorf("%s: no declared call %s", want.what, site)
		}
		if controlCalls[site] {
			t.Errorf("%s: the control holds %s", want.what, site)
		}
	}

	// The open calls are the two calls with no written callee and the call of the local
	// function value bump, whose reference the engine drops; none is a conversion or builtin.
	var open []string
	for _, g := range s.gaps {
		if g["gap"] == "open-call" {
			open = append(open, fmt.Sprintf("%s %v", g["file"], g["range"]))
		}
	}
	if want := []string{"flow.go [63 87 63 97]", "flow.go [63 99 63 106]", "flow.go [121 8 121 12]"}; !reflect.DeepEqual(open, want) {
		t.Errorf("open-call gaps %v, want %v", open, want)
	}

	// Both forms of implements, as scip-go writes them; Circle implements Shape although only
	// its value has Score, and the control holds the same rows.
	implements := func(dir string) []string {
		var out []string
		for _, r := range jsonl(t, filepath.Join(dir, "relations.jsonl")) {
			if r["rel"] == "implements" {
				out = append(out, r["from"].(string)+" -> "+r["to"].(string))
			}
		}
		sort.Strings(out)
		return out
	}
	wantImplements := []string{
		flowKey + "EmbeddedPointer# -> " + flowKey + "Loader#",
		flowKey + "EmbeddedValue# -> " + flowKey + "Shape#",
		meterKey + "Circle# -> " + flowKey + "Shape#",
		meterKey + "Circle#Score(). -> " + flowKey + "Shape#Score.",
		meterKey + "Gauge# -> " + flowKey + "Loader#",
		meterKey + "Gauge#Load(). -> " + flowKey + "Loader#Load.",
	}
	sort.Strings(wantImplements)
	if got := implements(out); !reflect.DeepEqual(got, wantImplements) {
		t.Errorf("implements\n got %v\nwant %v", got, wantImplements)
	}
	if got := implements(control); !reflect.DeepEqual(got, wantImplements) {
		t.Errorf("control implements %v", got)
	}

	overlap := false
	for _, g := range s.gaps {
		overlap = overlap || g["gap"] == "overlapping-bodies" &&
			reflect.DeepEqual(g["keys"], []any{flowKey + "A.", flowKey + "B."})
	}
	if !overlap {
		t.Errorf("no overlapping-bodies gap for A and B")
	}
	if got, want := s.skippedGaps(), []string{
		"base cgo.go build-constraint", "base ignored.go build-constraint", "base ignored/ignored.go build-constraint",
	}; !reflect.DeepEqual(got, want) {
		t.Errorf("skipped-file gaps %v, want %v", got, want)
	}
}

// callSites are a snapshot's declared calls, one per evidence site, as "from -> to at line:col".
func callSites(t *testing.T, dir string) map[string]bool {
	t.Helper()
	sites := map[string]bool{}
	for _, r := range jsonl(t, filepath.Join(dir, "relations.jsonl")) {
		if r["rel"] != "call" || r["resolution"] != "declared" {
			continue
		}
		evidence, _ := r["evidence"].([]any)
		for _, e := range evidence {
			ev := e.(map[string]any)
			sites[fmt.Sprintf("%s -> %s at %v:%v", r["from"], r["to"], ev["line"], ev["col"])] = true
		}
	}
	return sites
}

// reversed is the fixture's index with its documents, and each document's symbols, in reverse
// order.
func reversed(t *testing.T) []byte {
	t.Helper()
	idx := flowIndex(t)
	slices.Reverse(idx.Documents)
	for _, d := range idx.Documents {
		slices.Reverse(d.Symbols)
	}
	return marshal(t, &scip.Index{Metadata: idx.Metadata, Documents: idx.Documents, ExternalSymbols: idx.ExternalSymbols})
}

func TestDeterminism(t *testing.T) {
	w := newWorld(t)
	root := w.flow("go-flow")
	var outs []string
	for range 2 {
		out, code, stderr := w.index(w.caller(nil), root)
		if code != 0 {
			t.Fatalf("exit %d: %s", code, stderr)
		}
		outs = append(outs, out)
	}
	for _, name := range []string{"result.json", "syntax.jsonl", "build.jsonl"} {
		a, _ := os.ReadFile(filepath.Join(outs[0], name))
		b, _ := os.ReadFile(filepath.Join(outs[1], name))
		if len(a) == 0 || !bytes.Equal(a, b) {
			t.Errorf("two runs give different %s", name)
		}
	}

	served := filepath.Join(w.dir, "reversed.scip")
	if err := os.WriteFile(served, reversed(t), 0o644); err != nil {
		t.Fatal(err)
	}
	w.stub(served)
	out, code, stderr := w.index(w.caller(nil), root)
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}
	for _, name := range []string{"result.json", "syntax.jsonl", "build.jsonl"} {
		a, _ := os.ReadFile(filepath.Join(outs[0], name))
		b, _ := os.ReadFile(filepath.Join(out, name))
		if !bytes.Equal(a, b) {
			t.Errorf("the reversed index gives a different %s", name)
		}
	}
	snap := filepath.Join(w.dir, "reversed-snapshot")
	if code, stderr := w.engine("index", "--root", root, "--out", snap); code != 0 {
		t.Fatalf("gosanja index exit %d: %s", code, stderr)
	}
	if differ := sameTree(t, snap, filepath.Join(repo(t), "fixtures", "go-flow", "expected")); len(differ) != 0 {
		t.Errorf("the reversed index gives a snapshot that differs in %v", differ)
	}
}
