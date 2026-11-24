package base

import (
	"os"
	"path/filepath"
	"testing"
)

func TestIgnoredDir(t *testing.T) {
	for _, c := range []struct {
		dir      string
		patterns []string
		want     bool
	}{
		{"skipme", []string{"./skipme"}, true},
		{"skipme/inner", []string{"./skipme"}, true},
		{"pkg/skipme", []string{"./skipme"}, false},
		{"generated", []string{"generated"}, true},
		{"pkg/generated", []string{"generated"}, true},
		{"pkg/generated/inner", []string{"generated"}, true},
		{"pkg/generatedx", []string{"generated"}, false},
		{"a/b", []string{"a/b"}, true},
		{"x/a/b/c", []string{"a/b"}, true},
		{"pkg", []string{"generated", "./other"}, false},
	} {
		if got := ignoredDir(c.dir, c.patterns); got != c.want {
			t.Errorf("ignoredDir(%q, %q) = %v, want %v", c.dir, c.patterns, got, c.want)
		}
	}
}

func TestReleaseTags(t *testing.T) {
	for word, last := range map[string]string{"go1.26.3": "go1.26", "go1.27.3-X:nodwarf5": "go1.27", "go1.21": "go1.21", "go1.9rc1": "go1.9"} {
		tags, ok := releaseTags(word)
		if !ok || len(tags) == 0 || tags[0] != "go1.1" || tags[len(tags)-1] != last {
			t.Errorf("releaseTags(%q) = %v, %v", word, tags, ok)
		}
	}
	for _, word := range []string{"devel", "go2.0", "go1.", "go1.x"} {
		if tags, ok := releaseTags(word); ok {
			t.Errorf("releaseTags(%q) = %v", word, tags)
		}
	}
}

// The fallback judges a version-constrained file by the listing go command's release, not by
// the compiler that built the adapter.
func TestBuildContextRelease(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "a.go"), []byte("//go:build go1.27\n\npackage p\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	ctxt := buildContext("go1.26.3")
	pkg, _ := ctxt.ImportDir(dir, 0)
	if pkg == nil || len(pkg.IgnoredGoFiles) != 1 || len(pkg.GoFiles) != 0 {
		t.Fatalf("go1.26.3: %+v", pkg)
	}
	ctxt = buildContext("go1.27.3")
	if pkg, _ := ctxt.ImportDir(dir, 0); pkg == nil || len(pkg.GoFiles) != 1 {
		t.Fatalf("go1.27.3: %+v", pkg)
	}
}
