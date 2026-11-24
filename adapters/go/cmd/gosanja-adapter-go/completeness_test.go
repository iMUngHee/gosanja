package main

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"

	"gosanja/adapters/go/internal/scipindex"
)

// flowIndex is the go-flow fixture's checked-in index, decoded.
func flowIndex(t *testing.T) *scip.Index {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(repo(t), "fixtures", "go-flow", "index.scip"))
	if err != nil {
		t.Fatal(err)
	}
	var idx scip.Index
	if err := proto.Unmarshal(data, &idx); err != nil {
		t.Fatal(err)
	}
	return &idx
}

func marshal(t *testing.T, idx *scip.Index) []byte {
	t.Helper()
	data, err := proto.Marshal(idx)
	if err != nil {
		t.Fatal(err)
	}
	return data
}

// without is the fixture's index with the documents of the given paths removed.
func without(t *testing.T, paths ...string) []byte {
	t.Helper()
	idx := flowIndex(t)
	drop := map[string]bool{}
	for _, p := range paths {
		drop[p] = true
	}
	var kept []*scip.Document
	found := 0
	for _, d := range idx.Documents {
		if drop[d.RelativePath] {
			found++
			continue
		}
		kept = append(kept, d)
	}
	if found != len(paths) {
		t.Fatalf("the fixture's index lacks one of %v", paths)
	}
	idx.Documents = kept
	return marshal(t, idx)
}

// varint encodes n as a protobuf varint.
func varint(n uint64) []byte {
	var out []byte
	for n >= 0x80 {
		out = append(out, byte(n)|0x80)
		n >>= 7
	}
	return append(out, byte(n))
}

// oversized is a message whose field declares a length of 2^40 bytes, of which 16 follow.
func oversized(field byte) []byte {
	return append(append([]byte{field<<3 | 2}, varint(1<<40)...), make([]byte, 16)...)
}

func TestCompleteness(t *testing.T) {
	checked, err := os.ReadFile(filepath.Join(repo(t), "fixtures", "go-flow", "index.scip"))
	if err != nil {
		t.Fatal(err)
	}
	idx := flowIndex(t)
	half := len(idx.Documents) / 2
	first := &scip.Index{Metadata: idx.Metadata, Documents: idx.Documents[:half]}
	second := &scip.Index{Documents: idx.Documents[half:], ExternalSymbols: idx.ExternalSymbols}
	concatenated := append(marshal(t, first), marshal(t, second)...)

	cases := []struct {
		name, code string
		index      []byte
	}{
		{"a package file missing", "index-incomplete", without(t, "flow.go")},
		{"the external test missing", "index-incomplete", without(t, "external_test.go")},
		{"no document", "index-incomplete", marshal(t, &scip.Index{Metadata: idx.Metadata})},
		{"truncated", "index-unreadable", checked[:len(checked)/2]},
		{"oversized metadata", "index-unreadable", oversized(1)},
		{"oversized document", "index-unreadable", oversized(2)},
		{"oversized external symbol", "index-unreadable", oversized(3)},
		{"two concatenated messages", "", concatenated},
		{"the checked-in index", "", checked},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			w := newWorld(t)
			root := w.flow("go-flow")
			served := filepath.Join(w.dir, "served.scip")
			if err := os.WriteFile(served, c.index, 0o644); err != nil {
				t.Fatal(err)
			}
			w.stub(served)
			out, code, stderr := w.index(w.caller(nil), root)
			if code != 0 {
				t.Fatalf("exit %d: %s", code, stderr)
			}
			entry := baseEntry(t, out)
			_, published := os.Stat(filepath.Join(out, "index.scip"))
			if c.code == "" {
				if entry.Status != "ok" || published != nil {
					t.Fatalf("base %+v, index published: %v; stderr %s", entry, published == nil, stderr)
				}
				return
			}
			if entry.Status != "failed" || len(entry.Diagnostics) != 1 || entry.Diagnostics[0].Code != c.code {
				t.Fatalf("base %+v, want %s; stderr %s", entry, c.code, stderr)
			}
			if published == nil {
				t.Fatal("an index was published for a failed base layer")
			}
		})
	}
}

// The decoder never allocates a declared length that the bytes do not hold.
func TestDecoderAllocation(t *testing.T) {
	for _, field := range []byte{1, 2, 3} {
		var before, after runtime.MemStats
		runtime.GC()
		runtime.ReadMemStats(&before)
		if _, err := scipindex.Decode(oversized(field)); err == nil {
			t.Fatalf("field %d: decoded", field)
		}
		runtime.ReadMemStats(&after)
		if grown := after.TotalAlloc - before.TotalAlloc; grown >= 64<<20 {
			t.Fatalf("field %d: %d bytes allocated", field, grown)
		}
	}
}
