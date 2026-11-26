package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"

	"gosanja/adapters/go/internal/base"
	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/scipindex"
)

func TestBuildLayer(t *testing.T) {
	t.Run("GoFlow", testBuildGoFlow)
	t.Run("InventoryVariants", testBuildInventoryVariants)
	t.Run("TestOnlyRoots", testBuildTestOnlyRoots)
	t.Run("MergedInternalTests", testBuildMergedInternalTests)
	t.Run("TaggedSingleFile", testBuildTaggedSingleFile)
	t.Run("RetainedInvalidFile", testBuildRetainedInvalidFile)
	t.Run("Selection", testBuildSelection)
}

func testBuildSelection(t *testing.T) {
	for _, tc := range []struct {
		list  string
		build bool
	}{
		{"", true},
		{"base", false},
		{"base,syntax", false},
		{"base,build", true},
	} {
		name := tc.list
		if name == "" {
			name = "default"
		}
		t.Run(name, func(t *testing.T) {
			w := newWorld(t)
			var args []string
			if tc.list != "" {
				args = []string{"--layers", tc.list}
			}
			_, snapshot := flowSnapshot(t, w, args...)
			state := "not-requested"
			if tc.build {
				state = "present"
			}
			if got := readSnapshot(t, snapshot).layers["build"]; got != state {
				t.Errorf("build state = %s, want %s", got, state)
			}
			targets, err := os.ReadFile(filepath.Join(snapshot, "targets.jsonl"))
			if err != nil || (len(targets) != 0) != tc.build {
				t.Errorf("targets.jsonl: %v, %d bytes, build requested = %t", err, len(targets), tc.build)
			}
			for _, table := range []string{"files", "entities"} {
				marks := 0
				for _, row := range jsonl(t, filepath.Join(snapshot, table+".jsonl")) {
					if mark := row["test"]; mark != nil {
						marks++
						if mark != "file" {
							t.Errorf("%s row marked %v: %v", table, mark, row)
						}
					}
				}
				if (marks > 0) != tc.build {
					t.Errorf("%s rows marked as tests = %d, build requested = %t", table, marks, tc.build)
				}
			}
		})
	}
}

func targetID(module, pkg, kind string) string { return "gomod:" + module + ":" + pkg + ":" + kind }

func packageEntity(module, pkg string) string { return "gomod:" + module + ":`" + pkg + "`/" }

func rowsBy(t *testing.T, dir, table, field string) map[string]map[string]any {
	t.Helper()
	rows := map[string]map[string]any{}
	for _, row := range jsonl(t, filepath.Join(dir, table+".jsonl")) {
		key, ok := row[field].(string)
		if !ok || key == "" || rows[key] != nil {
			t.Fatalf("%s has a missing or repeated %s: %v", table, field, row)
		}
		rows[key] = row
	}
	return rows
}

func buildSnapshot(t *testing.T, w *world, root string, args ...string) string {
	t.Helper()
	out := filepath.Join(w.dir, "snapshot"+strings.Join(args, "-"))
	if code, stderr := w.engine(append([]string{"index", "--root", root, "--out", out}, args...)...); code != 0 {
		t.Fatalf("gosanja index exit %d: %s", code, stderr)
	}
	if code, stderr := w.engine("validate", out, "--root", root); code != 0 {
		t.Fatalf("gosanja validate exit %d: %s", code, stderr)
	}
	return out
}

func assertBuildTargets(t *testing.T, snapshot string, want map[string]string) {
	t.Helper()
	got := rowsBy(t, snapshot, "targets", "key")
	if len(got) != len(want) {
		t.Errorf("targets = %v, want keys and entities %v", got, want)
	}
	for key, entity := range want {
		row := got[key]
		_, rest, _ := strings.Cut(strings.TrimPrefix(key, "gomod:"), ":")
		name, kind, _ := strings.Cut(rest, ":")
		var test any
		if kind == "test" {
			test = true
		}
		if row["entity"] != entity || row["name"] != name || row["kind"] != kind || row["test"] != test {
			t.Errorf("target %q = %v, want entity %q and test %v", key, row, entity, test)
		}
	}
}

func assertFileBuild(t *testing.T, row map[string]any, test, constraint any, keys ...string) {
	t.Helper()
	var targets any
	if len(keys) != 0 {
		values := make([]any, len(keys))
		for i, key := range keys {
			values[i] = key
		}
		targets = values
	}
	if row == nil || !reflect.DeepEqual(row["targets"], targets) || row["test"] != test || row["constraint"] != constraint {
		t.Errorf("file row = %v, want targets %v, test %v, constraint %v", row, targets, test, constraint)
	}
}

func testBuildGoFlow(t *testing.T) {
	w := newWorld(t)
	_, snapshot := flowSnapshot(t, w)
	const module = "example.com/goflow"
	prod := targetID(module, module, "package")
	test := targetID(module, module, "test")
	meter := targetID(module, module+"/meter", "package")
	assertBuildTargets(t, snapshot, map[string]string{prod: flowKey, test: flowKey, meter: meterKey})
	assertMergedPackage(t, snapshot, flowKey, []string{"flow.go", "flow_test.go", "tagged.go", "zz_generated.go"})
	entities := rowsBy(t, snapshot, "entities", "key")
	if pkg := entities[flowKey]; pkg == nil || pkg["test"] != nil {
		t.Errorf("mixed production and test package = %v, want no test mark", pkg)
	}
	if def, _ := entities[meterKey]["def"].(map[string]any); def["file"] != "meter/meter.go" {
		t.Errorf("meter package definition = %v", def)
	}
	files := rowsBy(t, snapshot, "files", "path")
	for _, file := range []string{"flow.go", "zz_generated.go"} {
		assertFileBuild(t, files[file], nil, nil, prod, test)
	}
	assertFileBuild(t, files["tagged.go"], nil, "//go:build go1.21", prod, test)
	for _, file := range []string{"flow_test.go", "external_test.go"} {
		assertFileBuild(t, files[file], "file", nil, test)
	}
	assertFileBuild(t, files["meter/meter.go"], nil, nil, meter)
	testDeclarations := map[string]int{"flow_test.go": 0, "external_test.go": 0}
	for _, e := range entities {
		def, _ := e["def"].(map[string]any)
		file, _ := def["file"].(string)
		switch file {
		case "flow_test.go", "external_test.go":
			testDeclarations[file]++
			if e["test"] != "file" {
				t.Errorf("test declaration has no file mark: %v", e)
			}
		case "flow.go", "tagged.go", "zz_generated.go", "meter/meter.go":
			if e["test"] != nil {
				t.Errorf("production declaration marked as test: %v", e)
			}
		}
	}
	for file, n := range testDeclarations {
		if n == 0 {
			t.Errorf("no declaration from %s to test", file)
		}
	}
	if files["ignored.go"] != nil || !slices.Contains(readSnapshot(t, snapshot).skippedGaps(), "base ignored.go build-constraint") {
		t.Errorf("ignored.go admission or base diagnostic changed: %v", files["ignored.go"])
	}
}

func testBuildInventoryVariants(t *testing.T) {
	w := newWorld(t)
	const module = "example.com/variants"
	root := w.simple("variants", map[string]string{
		"a.go":               "",
		"main.go":            "package main\nfunc main() {}\n",
		"main_test.go":       "package main\nfunc helper() {}\n",
		"p/p.go":             "package custom\nfunc Value() int { return 1 }\n",
		"p/inside_test.go":   "package custom\nfunc helper() {}\n",
		"p/outside_test.go":  "package custom_test\nimport \"" + module + "/dep\"\nfunc helper() { _ = dep.Value() }\n",
		"dep/dep.go":         "package dep\nimport \"" + module + "/p\"\nfunc Value() int { return custom.Value() }\n",
		"ordinary.test/p.go": "package ordinary\nfunc V() {}\n",
	})
	synthIndexWithExternalTests(t, root, module, nil, []string{"p/outside_test.go"})
	inventoryPath := filepath.Join(w.dir, "go-inventory.jsonl")
	w.fakeGo(fmt.Sprintf(`if [ "$1" = list ] && [ "$2" = -e ] && [ "$3" = -test ] && [ "$4" = -deps ]; then
  "REALGO" "$@" > %q || exit $?
  cat %q
  exit $?
fi
exec "REALGO" "$@"`, inventoryPath, inventoryPath))
	out, code, stderr := w.index(w.caller(nil), root, "--layers", "base,build")
	if code != 0 || result(t, out).Layers["build"].Status != "ok" {
		t.Fatalf("adapter exit %d: %s", code, stderr)
	}
	data, err := os.ReadFile(inventoryPath)
	if err != nil {
		t.Fatal(err)
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	var recompiled, runner, realDotTest *base.Package
	for {
		var p base.Package
		if err := decoder.Decode(&p); errors.Is(err, io.EOF) {
			break
		} else if err != nil {
			t.Fatal(err)
		}
		switch p.ImportPath {
		case module + "/dep [" + module + "/p.test]":
			recompiled = &p
		case module + "/p.test":
			runner = &p
		case module + "/ordinary.test":
			realDotTest = &p
		}
	}
	if recompiled == nil || recompiled.ForTest != module+"/p" || runner == nil || len(runner.GoFiles) != 1 ||
		!(filepath.IsAbs(runner.GoFiles[0]) || runner.GoFiles[0] == "_testmain.go") ||
		realDotTest == nil || !slices.Equal(realDotTest.GoFiles, []string{"p.go"}) {
		t.Fatalf("listing missed variant, runner or real .test package: %v, %v, %v", recompiled, runner, realDotTest)
	}

	duplicate := filepath.Join(w.dir, "repeated-inventory.jsonl")
	w.write(duplicate, string(data)+string(data), 0o644)
	fakeBuildInventory(w, duplicate, filepath.Join(w.dir, "inventory-used"))
	again, code, stderr := w.index(w.caller(nil), root, "--layers", "base,build")
	if code != 0 || result(t, again).Layers["build"].Status != "ok" {
		t.Fatalf("adapter with repeated records exit %d: %s", code, stderr)
	}
	first, err := os.ReadFile(filepath.Join(out, "build.jsonl"))
	if err != nil {
		t.Fatal(err)
	}
	second, err := os.ReadFile(filepath.Join(again, "build.jsonl"))
	if err != nil || !bytes.Equal(first, second) {
		t.Fatalf("repeated records changed build rows: %v\n%s\n%s", err, first, second)
	}

	snapshot := buildSnapshot(t, w, root)
	command := targetID(module, module, "command")
	commandTest := targetID(module, module, "test")
	prod := targetID(module, module+"/p", "package")
	test := targetID(module, module+"/p", "test")
	dep := targetID(module, module+"/dep", "package")
	dotTest := targetID(module, module+"/ordinary.test", "package")
	assertBuildTargets(t, snapshot, map[string]string{
		command: packageEntity(module, module), commandTest: packageEntity(module, module),
		prod: packageEntity(module, module+"/p"), test: packageEntity(module, module+"/p"),
		dep: packageEntity(module, module+"/dep"), dotTest: packageEntity(module, module+"/ordinary.test"),
	})
	files := rowsBy(t, snapshot, "files", "path")
	if len(files) != 7 {
		t.Errorf("files = %v, want seven source files", files)
	}
	assertFileBuild(t, files["main.go"], nil, nil, command, commandTest)
	assertFileBuild(t, files["main_test.go"], "file", nil, commandTest)
	assertFileBuild(t, files["p/p.go"], nil, nil, prod, test)
	assertFileBuild(t, files["p/inside_test.go"], "file", nil, test)
	assertFileBuild(t, files["p/outside_test.go"], "file", nil, test)
	assertFileBuild(t, files["dep/dep.go"], nil, nil, dep)
	assertFileBuild(t, files["ordinary.test/p.go"], nil, nil, dotTest)
}

func testBuildTestOnlyRoots(t *testing.T) {
	w := newWorld(t)
	const module = "example.com/testonly"
	root := w.simple("testonly", map[string]string{
		"a.go":               "",
		"internal/a_test.go": "package internal\nfunc helper() {}\n",
		"external/a_test.go": "package custom_test\nfunc helper() {}\n",
		"mixed/a_test.go":    "package mixed\nfunc helper() {}\n",
		"mixed/b_test.go":    "package mixed_test\nfunc helper() {}\n",
	})
	synthIndexWithExternalTests(t, root, module, nil, []string{"external/a_test.go", "mixed/b_test.go"})
	adapterOut, code, stderr := w.index(w.caller(nil), root, "--layers", "base,build")
	if code != 0 {
		t.Fatalf("adapter exit %d: %s", code, stderr)
	}
	wantExternal := map[string]any{"symbol": "scip-go gomod " + module + " v0.0.0 `" + module + "/external_test`/"}
	var externalRoot any
	for _, row := range jsonl(t, filepath.Join(adapterOut, "build.jsonl")) {
		if row["key"] == targetID(module, module+"/external", "test") {
			externalRoot = row["root"]
		}
	}
	if !reflect.DeepEqual(externalRoot, wantExternal) {
		t.Errorf("adapter's external-only root = %v, want %v", externalRoot, wantExternal)
	}
	snapshot := buildSnapshot(t, w, root)
	internal := targetID(module, module+"/internal", "test")
	external := targetID(module, module+"/external", "test")
	mixed := targetID(module, module+"/mixed", "test")
	assertBuildTargets(t, snapshot, map[string]string{
		internal: packageEntity(module, module+"/internal"),
		external: packageEntity(module, module+"/external_test"),
		mixed:    packageEntity(module, module+"/mixed"),
	})
	files := rowsBy(t, snapshot, "files", "path")
	assertFileBuild(t, files["internal/a_test.go"], "file", nil, internal)
	assertFileBuild(t, files["external/a_test.go"], "file", nil, external)
	assertFileBuild(t, files["mixed/a_test.go"], "file", nil, mixed)
	assertFileBuild(t, files["mixed/b_test.go"], "file", nil, mixed)
	if countGaps(readSnapshot(t, snapshot), "unresolved-root", "build") != 0 {
		t.Error("a test-only target has an unresolved root")
	}
}

func testBuildMergedInternalTests(t *testing.T) {
	w := newWorld(t)
	const module = "example.com/alltest"
	root := w.simple("alltest", map[string]string{
		"a.go":      "",
		"a_test.go": "package p\nfunc helperA() {}\n",
		"b_test.go": "package p\nfunc helperB() {}\n",
	})
	synthIndex(t, root, module, nil)
	out, code, stderr := w.index(w.caller(nil), root)
	if code != 0 {
		t.Fatalf("adapter exit %d: %s", code, stderr)
	}
	kept := []string{"a_test.go", "b_test.go"}
	layers := result(t, out).Layers
	index, err := scipindex.Read(filepath.Join(out, "index.scip"))
	if err != nil {
		t.Fatal(err)
	}
	symbol := "scip-go gomod " + module + " v0.0.0 `" + module + "`/"
	if !slices.Equal(index.Paths(), kept) || !slices.Equal(layers["base"].Merged, []string{symbol}) ||
		layers["build"].Status != "ok" {
		t.Fatalf("adapter kept %v, result layers %v", index.Paths(), layers)
	}
	target := targetID(module, module, "test")
	var targets, filesWritten []string
	for _, row := range jsonl(t, filepath.Join(out, "build.jsonl")) {
		switch row["row"] {
		case "file":
			filesWritten = append(filesWritten, row["file"].(string))
			assertFileBuild(t, row, "file", nil, target)
		case "target":
			targets = append(targets, row["key"].(string))
		}
	}
	if !slices.Equal(targets, []string{target}) || !slices.Equal(filesWritten, kept) {
		t.Errorf("adapter targets = %v and files = %v, want %s and %v", targets, filesWritten, target, kept)
	}
	helpers := map[string]bool{}
	for _, row := range jsonl(t, filepath.Join(out, "syntax.jsonl")) {
		a := row["anchor"].(map[string]any)
		file := a["file"].(string)
		if a["line"] == float64(0) && a["col"] == float64(8) {
			t.Errorf("unexpected package-clause syntax row: %v", row)
		}
		if a["line"] == float64(1) && a["col"] == float64(5) {
			helpers[file] = true
		}
	}
	for _, file := range kept {
		if !helpers[file] {
			t.Errorf("missing helper syntax row in %s", file)
		}
	}
	snapshot := buildSnapshot(t, w, root)
	key := packageEntity(module, module)
	assertMergedPackage(t, snapshot, key, kept)
	assertBuildTargets(t, snapshot, map[string]string{target: key})
	entities := rowsBy(t, snapshot, "entities", "key")
	pkg := entities[key]
	if pkg["test"] != "file" || pkg["group"] != nil || pkg["member_of"] != nil {
		t.Errorf("merged package = %v, want file test mark and no collision", pkg)
	}
	for _, row := range entities {
		if row["member_of"] == key {
			t.Errorf("unexpected member of package collision: %v", row)
		}
	}
	files := rowsBy(t, snapshot, "files", "path")
	for _, file := range kept {
		assertFileBuild(t, files[file], "file", nil, target)
	}
}

func testBuildTaggedSingleFile(t *testing.T) {
	w := newWorld(t)
	const module = "example.com/tagged"
	const tag = "\t//go:build\tgo1.21  "
	root := w.simple("tagged", map[string]string{
		"a.go":           tag + "\r\n\r\npackage p\r\nfunc Value() {}\r\n",
		"excluded.go":    "//go:build !go1.21\n\npackage p\n",
		"block/block.go": "/*\n//go:build go1.21\n*/\npackage block\n",
	})
	synthIndex(t, root, module, nil)
	key := packageEntity(module, module)
	for _, tc := range []struct {
		name   string
		args   []string
		build  string
		syntax string
	}{
		{"default", nil, "present", "present"},
		{"build_only", []string{"--layers", "base,build"}, "present", "not-requested"},
		{"syntax_only", []string{"--layers", "base,syntax"}, "not-requested", "present"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			out := buildSnapshot(t, w, root, tc.args...)
			s := readSnapshot(t, out)
			if s.layers["build"] != tc.build || s.layers["syntax"] != tc.syntax {
				t.Errorf("layers = %v, want build %s and syntax %s", s.layers, tc.build, tc.syntax)
			}
			entities := rowsBy(t, out, "entities", "key")
			if def, _ := entities[key]["def"].(map[string]any); def["file"] != "a.go" {
				t.Errorf("tagged package def = %v", def)
			}
			if row := rowsBy(t, out, "syntax", "key")[key]; row != nil {
				t.Errorf("unexpected package-clause syntax row: %v", row)
			}
			files := rowsBy(t, out, "files", "path")
			if tc.build == "present" {
				assertFileBuild(t, files["a.go"], nil, tag, targetID(module, module, "package"))
			} else {
				assertFileBuild(t, files["a.go"], nil, nil)
			}
			if files["block/block.go"] == nil || files["block/block.go"]["constraint"] != nil || files["excluded.go"] != nil {
				t.Errorf("block or excluded file constraint leaked: %v", files)
			}
		})
	}
}

// fakeBuildInventory replaces only the base inventory call. The syntax loader and the
// other Go preconditions continue to use the real tool.
func fakeBuildInventory(w *world, inventoryPath, used string) {
	w.fakeGo(fmt.Sprintf(`if [ "$#" = 6 ] && [ "$1" = list ] && [ "$2" = -e ] && [ "$3" = -test ] && [ "$4" = -deps ] && [ "$6" = ./... ]; then
  case "$5" in
  -json=*) : > %q; exec cat %q ;;
  esac
fi
exec "REALGO" "$@"`, used, inventoryPath))
}

func testBuildRetainedInvalidFile(t *testing.T) {
	for _, tc := range []struct {
		name       string
		header     string
		constraint any
		skips      []contract.Skipped
	}{
		{"one_directive", "//go:build go1.21\n", "//go:build go1.21", []contract.Skipped{{File: "unowned.go", Code: "target-unknown"}}},
		{"two_directives", "//go:build go1.21\n//go:build go1.22\n", nil, []contract.Skipped{
			{File: "unowned.go", Code: "constraint-unreadable"}, {File: "unowned.go", Code: "target-unknown"},
		}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			w := newWorld(t)
			const module = "example.com/retained"
			validSource := tc.header + "\npackage p\n"
			root := w.simple("retained", map[string]string{
				"a.go":        "package p\n",
				"unowned.go":  validSource,
				"excluded.go": "//go:build !go1.21\n\npackage p\n",
			})
			synthIndex(t, root, module, nil)
			w.write(filepath.Join(root, "unowned.go"), validSource+"@\n", 0o644)
			record := base.Package{
				ImportPath: module, Dir: root, Name: "p", Module: &base.Module{Main: true, Path: module},
				GoFiles: []string{"a.go"}, InvalidGoFiles: []string{"unowned.go"}, IgnoredGoFiles: []string{"excluded.go"},
			}
			data, err := json.Marshal(record)
			if err != nil {
				t.Fatal(err)
			}
			inventoryPath := filepath.Join(w.dir, "go-inventory.json")
			w.write(inventoryPath, string(data)+"\n", 0o644)
			used := filepath.Join(w.dir, "inventory-used")
			fakeBuildInventory(w, inventoryPath, used)

			for _, args := range [][]string{nil, {"--layers", "base,build"}} {
				out, code, stderr := w.index(w.caller(nil), root, args...)
				if code != 0 {
					t.Fatalf("%v: adapter exit %d: %s", args, code, stderr)
				}
				if _, err := os.Stat(used); err != nil {
					t.Fatalf("%v: fake inventory call did not run: %v", args, err)
				}
				layers := result(t, out).Layers
				index, err := scipindex.Read(filepath.Join(out, "index.scip"))
				if err != nil {
					t.Fatal(err)
				}
				if !slices.Equal(index.Paths(), []string{"a.go", "unowned.go"}) ||
					!slices.Contains(layers["base"].Skipped, contract.Skipped{File: "unowned.go", Code: "parse-failed"}) ||
					layers["build"].Status != "ok" || !slices.Equal(layers["build"].Skipped, tc.skips) {
					t.Fatalf("%v: retained %v, layers %v", args, index.Paths(), layers)
				}
				target := targetID(module, module, "package")
				adapterFiles := map[string]map[string]any{}
				for _, row := range jsonl(t, filepath.Join(out, "build.jsonl")) {
					if row["row"] == "file" {
						adapterFiles[row["file"].(string)] = row
					}
				}
				assertFileBuild(t, adapterFiles["a.go"], nil, nil, target)
				if tc.constraint != nil {
					assertFileBuild(t, adapterFiles["unowned.go"], nil, tc.constraint)
				} else if adapterFiles["unowned.go"] != nil {
					t.Errorf("invalid constraint produced a file fact: %v", adapterFiles["unowned.go"])
				}
				if adapterFiles["excluded.go"] != nil {
					t.Errorf("excluded.go got a build row: %v", adapterFiles["excluded.go"])
				}
				if err := os.Remove(used); err != nil {
					t.Fatal(err)
				}

				snapshot := buildSnapshot(t, w, root, args...)
				if _, err := os.Stat(used); err != nil {
					t.Fatalf("%v: engine's adapter did not consume the fake inventory: %v", args, err)
				}
				files := rowsBy(t, snapshot, "files", "path")
				assertFileBuild(t, files["a.go"], nil, nil, target)
				assertFileBuild(t, files["unowned.go"], nil, tc.constraint)
				gaps := readSnapshot(t, snapshot).skippedGaps()
				if files["excluded.go"] != nil || !slices.Contains(gaps, "base excluded.go build-constraint") {
					t.Errorf("excluded.go file row or base skip changed: files %v, gaps %v", files, gaps)
				}
				var buildSkips []string
				for _, gap := range gaps {
					if strings.HasPrefix(gap, "build ") {
						buildSkips = append(buildSkips, gap)
					}
				}
				var wantSkips []string
				for _, skip := range tc.skips {
					wantSkips = append(wantSkips, "build "+skip.File+" "+skip.Code)
				}
				if !slices.Equal(buildSkips, wantSkips) {
					t.Errorf("snapshot build skips %v, want %v", buildSkips, wantSkips)
				}
				if err := os.Remove(used); err != nil {
					t.Fatal(err)
				}
			}
		})
	}
}
