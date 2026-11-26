package base

import (
	"slices"
	"strings"
	"testing"

	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/env"
)

func TestFailureReportsDiagnosticDetail(t *testing.T) {
	var stderr strings.Builder
	outcome, err := Run(t.Context(), Config{GOOS: "windows", Stderr: &stderr})
	if err != nil || outcome.Entry.Status != "failed" {
		t.Fatalf("entry = %+v, err = %v", outcome.Entry, err)
	}
	const want = "platform-unsupported\nthe Go adapter does not run on Windows\n"
	if got := stderr.String(); got != want {
		t.Fatalf("stderr = %q, want %q", got, want)
	}
}

func TestSkippedFilesOrderDistinctCodes(t *testing.T) {
	first := contract.Skipped{File: "a.go", Code: "parse-failed"}
	second := contract.Skipped{File: "a.go", Code: "unreadable"}
	third := contract.Skipped{File: "z.go", Code: "parse-failed"}
	got := sortSkipped([]contract.Skipped{third, second, first, second})
	want := []contract.Skipped{first, second, third}
	if !slices.Equal(got, want) {
		t.Fatalf("sorted skips = %v, want %v", got, want)
	}
}

func TestEmptyGOPATHDoesNotInventCachePaths(t *testing.T) {
	root := t.TempDir()
	got, err := checkDestinations(Config{Root: root, Caller: env.Caller{Cwd: root}}, env.Values{})
	if err != nil || got["GOPATH"] != "" || got["GOMODCACHE"] != "" {
		t.Fatalf("resolved settings = %v, err = %v", got, err)
	}
}

func TestGoVersionWordWithNoPlatformSuffix(t *testing.T) {
	for input, want := range map[string]string{
		"other version go1.27.3": "other version go1.27.3",
		"go env go1.27.3":        "go env go1.27.3",
		"go version go1.27.3":    "go1.27.3",
		"go version":             "go version",
	} {
		if got := goWord(input); got != want {
			t.Errorf("goWord(%q) = %q, want %q", input, got, want)
		}
	}
}

func TestModuleFilesOrderedByRelativePath(t *testing.T) {
	root := t.TempDir()
	inventory := Inventory{Packages: []Package{{
		ImportPath: "example.com/p", Dir: root, Module: &Module{Main: true},
		GoFiles: []string{"z.go", "a.go"},
	}}}
	got := inventory.moduleFiles(root, sources)
	if len(got) != 2 || got[0].rel != "a.go" || got[1].rel != "z.go" {
		t.Fatalf("module files out of order: %v", got)
	}
}
