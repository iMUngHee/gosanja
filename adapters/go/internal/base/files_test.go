package base

import (
	"go/build"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"testing"

	"github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"
	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/env"
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

// The fallback judges a version-constrained file by the listing go command's release, not by
// the compiler that built the adapter.
func TestBuildContextRelease(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "a.go"), []byte("//go:build go1.27\n\npackage p\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	tool := filepath.Join(t.TempDir(), "go")
	if err := os.WriteFile(tool, []byte("#!/bin/sh\nprintf '%s\\n' '' \"$RELEASE_TAGS\"\n"), 0o700); err != nil {
		t.Fatal(err)
	}
	for _, test := range []struct {
		tags  string
		built bool
	}{
		{"go1.1 go1.26", false},
		{"go1.1 go1.26 go1.27", true},
	} {
		ctxt, err := buildContext(t.Context(), dir, tool, []string{"RELEASE_TAGS=" + test.tags})
		if err != nil {
			t.Fatal(err)
		}
		pkg, _ := ctxt.ImportDir(dir, 0)
		if pkg == nil || slices.Contains(pkg.GoFiles, "a.go") != test.built ||
			slices.Contains(pkg.IgnoredGoFiles, "a.go") == test.built {
			t.Fatalf("release tags %q: %+v", test.tags, pkg)
		}
	}
}

// archTag is, for an architecture, a variable that selects its feature level, a level for the
// adapter's process that sets a build tag, and a lower level for the go command that does not.
var archTag = map[string]struct{ variable, parent, child, tag string }{
	"amd64":    {"GOAMD64", "v3", "v1", "amd64.v3"},
	"arm64":    {"GOARM64", "v8.1", "v8.0", "arm64.v8.1"},
	"arm":      {"GOARM", "7", "5", "arm.7"},
	"386":      {"GO386", "sse2", "softfloat", "386.sse2"},
	"ppc64":    {"GOPPC64", "power9", "power8", "ppc64.power9"},
	"ppc64le":  {"GOPPC64", "power9", "power8", "ppc64le.power9"},
	"riscv64":  {"GORISCV64", "rva22u64", "rva20u64", "riscv64.rva22u64"},
	"mips":     {"GOMIPS", "hardfloat", "softfloat", "mips.hardfloat"},
	"mipsle":   {"GOMIPS", "hardfloat", "softfloat", "mipsle.hardfloat"},
	"mips64":   {"GOMIPS64", "hardfloat", "softfloat", "mips64.hardfloat"},
	"mips64le": {"GOMIPS64", "hardfloat", "softfloat", "mips64le.hardfloat"},
}

// The fallback judges files by the build tags of the go command that lists the packages, not
// by those of the adapter's process: the process runs with a higher architecture level than
// the go command it starts, and a directory whose only file needs that level is excluded.
func TestFallbackUsesSelectedGoToolTags(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Fatal("the adapter fails on Windows before the fallback runs; this test needs a Unix host")
	}
	arch, ok := archTag[runtime.GOARCH]
	if !ok {
		t.Fatalf("no architecture level variable is known for GOARCH=%s; add one to archTag", runtime.GOARCH)
	}
	const subprocess = "GOSANJA_BUILD_TAG_TEST_CHILD"
	if os.Getenv(subprocess) != "1" {
		cmd := exec.CommandContext(t.Context(), os.Args[0], "-test.run=^TestFallbackUsesSelectedGoToolTags$", "-test.v")
		cmd.Env = append(os.Environ(), subprocess+"=1", arch.variable+"="+arch.parent)
		if output, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("%v:\n%s", err, output)
		}
		return
	}
	if !slices.Contains(build.Default.ToolTags, arch.tag) {
		t.Fatalf("adapter process tags: %v, want %s among them", build.Default.ToolTags, arch.tag)
	}
	root, bin := t.TempDir(), t.TempDir()
	if err := os.Mkdir(filepath.Join(root, "v3only"), 0o700); err != nil {
		t.Fatal(err)
	}
	for filename, content := range map[string]string{
		"go.mod":      "module example.com/tagtest\n\ngo 1.26\n",
		"ordinary.go": "package tagtest\nfunc Ordinary() {}\n",
		"v3only/only.go": "//go:build " + arch.tag + "\n\npackage v3only\n" +
			"func OnlyOnV3() {}\n",
	} {
		if err := os.WriteFile(filepath.Join(root, filename), []byte(content), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	realGo, err := exec.LookPath("go")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(realGo, filepath.Join(bin, "real-go")); err != nil {
		t.Fatal(err)
	}
	for name, script := range map[string]string{
		"go": "#!/bin/sh\nexport " + arch.variable + "=" + arch.child + "\nexec \"$(dirname \"$0\")/real-go\" \"$@\"\n",
		"scip-go": `#!/bin/sh
set -eu
case "$1" in
    --version) printf '0.2.10\n' ;;
    index) test "$4" = --output && cp recorded.scip "$5" ;;
    *) exit 1 ;;
esac
`,
	} {
		if err := os.WriteFile(filepath.Join(bin, name), []byte(script), 0o700); err != nil {
			t.Fatal(err)
		}
	}
	recorded, err := proto.Marshal(&scip.Index{Documents: []*scip.Document{{RelativePath: "ordinary.go"}}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "recorded.scip"), recorded, 0o600); err != nil {
		t.Fatal(err)
	}
	var stderr strings.Builder
	outcome, err := Run(t.Context(), Config{
		Root: root, Out: t.TempDir(), GOOS: runtime.GOOS, Stderr: &stderr,
		Caller: env.Caller{Cwd: root, Vars: map[string]string{
			"PATH": bin + string(os.PathListSeparator) + os.Getenv("PATH"), "HOME": t.TempDir(),
			"GOENV": "off", "GOPROXY": "off", "GONOPROXY": "none", "GOSUMDB": "off",
		}},
	})
	want := []contract.Skipped{{File: "v3only/only.go", Code: "build-constraint"}}
	if err != nil || outcome.Entry.Status != "ok" || !slices.Equal(outcome.Entry.Skipped, want) {
		t.Fatalf("outcome = %+v, err = %v, stderr = %s", outcome.Entry, err, stderr.String())
	}
}
