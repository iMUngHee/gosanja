package build

import (
	"bytes"
	"encoding/json"
	"slices"
	"strings"
	"testing"

	"gosanja/adapters/go/internal/base"
	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/scipindex"
)

func TestTargetKey(t *testing.T) {
	if got, want := targetKey("example.com/%p", "x/\t\u00a0\u0001\ufeff한", "test"), "gomod:example.com/%25p:x/%09%C2%A0%01%EF%BB%BF한:test"; got != want {
		t.Errorf("key = %q, want %q", got, want)
	}
}

func TestKnownTestFilesWithoutVariantMembership(t *testing.T) {
	const module = "example.com/p"
	const packageKey = "gomod:example.com/p:example.com/p:package"
	const testKey = "gomod:example.com/p:example.com/p:test"
	root := t.TempDir()
	outcome := base.Outcome{
		Inventory: &base.Inventory{Packages: []base.Package{{
			ImportPath: module, Dir: root, Name: "p", Module: &base.Module{Main: true, Path: module},
			GoFiles: []string{"p.go"}, TestGoFiles: []string{"p_test.go"}, XTestGoFiles: []string{"outside_test.go"},
		}}},
		Documents: map[string]bool{"p.go": true, "p_test.go": true, "outside_test.go": true},
		Index:     &scipindex.Index{},
	}
	var buffer bytes.Buffer
	entry, err := Layer(&buffer, root, outcome)
	skips := []contract.Skipped{
		{File: "outside_test.go", Code: "target-unknown"},
		{File: "p_test.go", Code: "target-unknown"},
	}
	if err != nil || entry.Status != "ok" || !slices.Equal(entry.Skipped, skips) {
		t.Fatalf("build = %+v, error = %v, want successful build with skips %v", entry, err, skips)
	}
	want := []string{
		`{"row":"target","key":"` + packageKey + `","kind":"package","name":"` + module + `"}`,
		`{"row":"target","key":"` + testKey + `","kind":"test","name":"` + module + `","test":true}`,
		`{"row":"file","file":"outside_test.go","test":"file"}`,
		`{"row":"file","file":"p.go","targets":["` + packageKey + `","` + testKey + `"]}`,
		`{"row":"file","file":"p_test.go","test":"file"}`,
	}
	if got := strings.Split(strings.TrimSuffix(buffer.String(), "\n"), "\n"); !slices.Equal(got, want) {
		t.Errorf("rows = %v, want %v", got, want)
	}
}

func TestTargetKeyByteLimit(t *testing.T) {
	for _, size := range []int{1024, 1025} {
		root := t.TempDir()
		const overhead = len("gomod:m::package")
		importPath := strings.Repeat("é", (size-overhead)/2) + strings.Repeat("x", (size-overhead)%2)
		outcome := base.Outcome{
			Inventory: &base.Inventory{Packages: []base.Package{{
				ImportPath: importPath, Dir: root, Name: "p", Module: &base.Module{Main: true, Path: "m"},
				GoFiles: []string{"p.go"},
			}}},
			Index: &scipindex.Index{},
		}
		var rows bytes.Buffer
		entry, err := Layer(&rows, root, outcome)
		if err != nil {
			t.Fatal(err)
		}
		if size == 1024 {
			var target map[string]json.RawMessage
			if err := json.Unmarshal(rows.Bytes(), &target); err != nil || entry.Status != "ok" || string(target["key"]) != `"gomod:m:`+importPath+`:package"` {
				t.Errorf("%d bytes: build = %+v, JSON error = %v, rows = %s", size, entry, err, &rows)
			}
		} else if entry.Status != "failed" || !slices.Equal(entry.Diagnostics, []contract.Diagnostic{{Code: "target-key-invalid"}}) || rows.Len() != 0 {
			t.Errorf("%d bytes: build = %+v, rows = %d bytes", size, entry, rows.Len())
		}
	}
}
