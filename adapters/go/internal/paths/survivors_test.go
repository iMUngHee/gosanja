package paths

import (
	"os"
	"path/filepath"
	"testing"
)

func TestResolveExistingRegularFile(t *testing.T) {
	root, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	want := filepath.Join(root, "file")
	if err := os.WriteFile(want, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	got, err := Resolve(want, root)
	if got != want || err != nil {
		t.Fatalf("resolved path = %q, err = %v; want %q", got, err, want)
	}
}
