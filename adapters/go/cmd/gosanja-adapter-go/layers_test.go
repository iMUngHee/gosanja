package main

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"

	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/syntax"
)

// recordLayer replaces the syntax layer for the test and counts its calls; the real layer
// still computes the result.
func recordLayer(t *testing.T) *int {
	t.Helper()
	calls := 0
	real := layer
	layer = func(ctx context.Context, in syntax.Input) (contract.Entry, []contract.SyntaxFact) {
		calls++
		return real(ctx, in)
	}
	t.Cleanup(func() { layer = real })
	return &calls
}

func TestLayers(t *testing.T) {
	cases := []struct {
		list   []string // nil: no --layers
		layers []string
	}{
		{[]string{"--layers", "base"}, []string{"base"}},
		{[]string{"--layers", "base,syntax"}, []string{"base", "syntax"}},
		{[]string{"--layers", "syntax,base"}, []string{"base", "syntax"}},
		{[]string{"--layers", "base,build"}, []string{"base", "build"}},
		{nil, []string{"base", "build", "syntax"}},
	}
	for _, c := range cases {
		name := "no --layers"
		if c.list != nil {
			name = c.list[1]
		}
		t.Run(name, func(t *testing.T) {
			calls := recordLayer(t)
			w := newWorld(t)
			root := w.flow("go-flow")
			out, code, stderr := w.index(w.caller(nil), root, c.list...)
			if code != 0 {
				t.Fatalf("exit %d: %s", code, stderr)
			}
			r := result(t, out)
			var got []string
			for name := range r.Layers {
				got = append(got, name)
			}
			slices.Sort(got)
			if !reflect.DeepEqual(got, c.layers) {
				t.Errorf("layers %v, want %v", got, c.layers)
			}
			_, err := os.Stat(filepath.Join(out, "syntax.jsonl"))
			syntaxWanted := slices.Contains(c.layers, "syntax")
			if syntaxWanted {
				if *calls != 1 || err != nil || r.Layers["syntax"].Status != "ok" {
					t.Fatalf("the layer ran %d times, syntax.jsonl: %v, syntax %+v", *calls, err, r.Layers["syntax"])
				}
				data, _ := os.ReadFile(filepath.Join(out, "syntax.jsonl"))
				rows := strings.Split(strings.TrimSuffix(string(data), "\n"), "\n")
				if len(data) == 0 || data[len(data)-1] != '\n' {
					t.Errorf("syntax.jsonl does not end its last row with a newline")
				}
				for _, row := range rows {
					validate(t, "syntax-fact.schema.json", []byte(row))
				}
			} else if *calls != 0 || err == nil {
				t.Errorf("the layer ran %d times, syntax.jsonl written: %v", *calls, err == nil)
			}
			buildData, err := os.ReadFile(filepath.Join(out, "build.jsonl"))
			if buildWanted := slices.Contains(c.layers, "build"); buildWanted {
				if err != nil || r.Layers["build"].Status != "ok" {
					t.Fatalf("build.jsonl: %v, build %+v", err, r.Layers["build"])
				}
				if len(buildData) == 0 || buildData[len(buildData)-1] != '\n' {
					t.Fatal("build.jsonl is empty or has no trailing newline")
				}
				for row := range bytes.SplitSeq(bytes.TrimSuffix(buildData, []byte{'\n'}), []byte{'\n'}) {
					validate(t, "build.schema.json", row)
				}
			} else if !os.IsNotExist(err) {
				t.Errorf("build.jsonl should be absent: %v", err)
			}

			if n := indexRuns(t, w); n != 1 {
				t.Errorf("scip-go ran %d times", n)
			}
		})
	}
}
