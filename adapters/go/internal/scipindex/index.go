// Package scipindex reads the index scip-go wrote. The file is decoded whole with
// proto.Unmarshal, which accepts concatenated Index messages and checks every declared length
// against the bytes that exist; the bindings' streaming parser allocates a declared length
// before checking it.
package scipindex

import (
	"fmt"
	"os"
	"sort"

	"github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"
)

// Index is what the adapter needs of an index: its documents.
type Index struct {
	Documents []*scip.Document
}

// Read decodes the index file at path.
func Read(path string) (*Index, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return Decode(data)
}

// Decode decodes index bytes, one or several concatenated Index messages.
func Decode(data []byte) (*Index, error) {
	var index scip.Index
	if err := proto.Unmarshal(data, &index); err != nil {
		return nil, fmt.Errorf("not a SCIP index: %w", err)
	}
	return &Index{Documents: index.Documents}, nil
}

// Keep rewrites the index file at path with only the documents whose path keep accepts,
// keeping its metadata and external symbols.
func Keep(path string, keep func(string) bool) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	var index scip.Index
	if err := proto.Unmarshal(data, &index); err != nil {
		return fmt.Errorf("not a SCIP index: %w", err)
	}
	var kept []*scip.Document
	for _, d := range index.Documents {
		if keep(d.RelativePath) {
			kept = append(kept, d)
		}
	}
	index.Documents = kept
	out, err := proto.MarshalOptions{Deterministic: true}.Marshal(&index)
	if err != nil {
		return err
	}
	return os.WriteFile(path, out, 0o644)
}

// Paths lists the documents' relative paths, sorted, each once.
func (x *Index) Paths() []string {
	seen := map[string]bool{}
	var paths []string
	for _, d := range x.Documents {
		if !seen[d.RelativePath] {
			seen[d.RelativePath] = true
			paths = append(paths, d.RelativePath)
		}
	}
	sort.Strings(paths)
	return paths
}

// Range is an occurrence range as [startLine, startCol, endLine, endCol].
type Range [4]int32

// OccurrenceRange gives an occurrence's range, the typed field over the legacy one, as
// spec/adapter-contract.md says. ok is false when the occurrence has neither.
func OccurrenceRange(o *scip.Occurrence) (r Range, ok bool) {
	if s := o.GetSingleLineRange(); s != nil {
		return Range{s.Line, s.StartCharacter, s.Line, s.EndCharacter}, true
	}
	if m := o.GetMultiLineRange(); m != nil {
		return Range{m.StartLine, m.StartCharacter, m.EndLine, m.EndCharacter}, true
	}
	return legacy(o.Range)
}

// EnclosingRange gives an occurrence's enclosing range, typed over legacy.
func EnclosingRange(o *scip.Occurrence) (r Range, ok bool) {
	if s := o.GetSingleLineEnclosingRange(); s != nil {
		return Range{s.Line, s.StartCharacter, s.Line, s.EndCharacter}, true
	}
	if m := o.GetMultiLineEnclosingRange(); m != nil {
		return Range{m.StartLine, m.StartCharacter, m.EndLine, m.EndCharacter}, true
	}
	return legacy(o.EnclosingRange)
}

func legacy(r []int32) (Range, bool) {
	switch len(r) {
	case 3:
		return Range{r[0], r[1], r[0], r[2]}, true
	case 4:
		return Range{r[0], r[1], r[2], r[3]}, true
	}
	return Range{}, false
}
