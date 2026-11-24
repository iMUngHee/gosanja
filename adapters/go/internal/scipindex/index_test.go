package scipindex

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	"github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"
)

func TestKeepPreservesDocumentProvenance(t *testing.T) {
	metadata := func(tool string) *scip.Index {
		return &scip.Index{Metadata: &scip.Metadata{ToolInfo: &scip.ToolInfo{Name: tool}}}
	}
	document := func(path string) *scip.Index {
		return &scip.Index{Documents: []*scip.Document{{RelativePath: path}}}
	}
	records := []*scip.Index{
		metadata("first-indexer"), document("first.go"), document("excluded.go"),
		metadata("second-indexer"), document("second.go"),
		{ExternalSymbols: []*scip.SymbolInformation{{Symbol: "external one"}}},
	}
	var input, want []byte
	for i, record := range records {
		encoded, err := proto.Marshal(record)
		if err != nil {
			t.Fatal(err)
		}
		input = append(input, encoded...)
		if i != 2 {
			want = append(want, encoded...)
		}
	}
	path := filepath.Join(t.TempDir(), "index.scip")
	if err := os.WriteFile(path, input, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := Keep(path, func(name string) bool { return name != "excluded.go" }); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, want) {
		t.Fatalf("filtering changed retained wire records\n got %x\nwant %x", got, want)
	}
}
