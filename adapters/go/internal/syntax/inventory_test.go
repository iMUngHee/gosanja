package syntax

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/scip-code/scip/bindings/go/scip"
	"golang.org/x/tools/go/packages"
	"google.golang.org/protobuf/proto"
	"gosanja/adapters/go/internal/base"
	"gosanja/adapters/go/internal/contract"
	adapterenv "gosanja/adapters/go/internal/env"
)

func TestOrdinaryDotTestPackage(t *testing.T) {
	const command = "feature.test/main.go"
	in := tempModule(t, map[string]string{
		command:          "package main\nfunc main() {}\n",
		"module.go":      "package p\nfunc F() {}\n",
		"module_test.go": "package p\nfunc helper() {}\n",
	}, "")
	missing := runInventoryFixture(t, in.Root, "module.go", "module_test.go")
	if missing.Entry.Status != "failed" || len(missing.Entry.Diagnostics) != 1 ||
		missing.Entry.Diagnostics[0].Code != "index-incomplete" {
		t.Errorf("index omitting %s returned %+v", command, missing.Entry)
	}

	complete := runInventoryFixture(t, in.Root, command, "module.go", "module_test.go")
	applyInventory(t, &in, complete)
	for id := range in.Inventory {
		if strings.HasSuffix(id, ".test") && !strings.HasSuffix(id, "/feature.test") {
			t.Errorf("generated test runner in ownership: %s", id)
		}
	}
	status, rows := Layer(t.Context(), in)
	wantLayer(t, status, rows, nil, []string{command, "module.go", "module_test.go"})
}

func TestDiagnosticInventoryMembership(t *testing.T) {
	const external = "external_test.go"
	for _, category := range []string{"ordinary", "internal test", "positioned"} {
		t.Run(category, func(t *testing.T) {
			in := tempModule(t, map[string]string{
				external:         "package p_test\n//line external.y:99\nfunc External() {}\n",
				"module.go":      "package p\nfunc F() {}\n",
				"module_test.go": "package p\nfunc helper() {}\n",
			}, "")
			out := runInventoryFixture(t, in.Root, external, "module.go", "module_test.go")
			applyInventory(t, &in, out)
			matched := false
			status, rows := loadLayer(t.Context(), in, func(cfg *packages.Config, patterns ...string) ([]*packages.Package, error) {
				variants, err := packages.Load(cfg, patterns...)
				for _, pkg := range variants {
					if strings.HasSuffix(pkg.PkgPath, "_test") || pkg.Name == "main" {
						continue
					}
					testVariant := strings.Contains(pkg.ID, " [")
					if testVariant != (category == "internal test") {
						continue
					}
					matched = true
					if category == "positioned" {
						pkg.Errors = append(pkg.Errors, packages.Error{Kind: packages.ListError,
							Pos: filepath.Join(in.Root, "module.go") + ":2:1", Msg: "positioned loader error"})
					} else {
						pkg.Errors = append(pkg.Errors, packages.Error{Kind: packages.ListError, Msg: "unpositioned loader error"})
					}
				}
				return variants, err
			})
			if !matched {
				t.Fatal("loader did not return the requested package variant")
			}
			skips := []contract.Skipped{{File: "module.go", Code: "package-error"}}
			files := []string{external, "module_test.go"}
			if category == "internal test" {
				skips = append(skips, contract.Skipped{File: "module_test.go", Code: "package-error"})
				files = []string{external}
			}
			if category == "positioned" {
				files = []string{external, "module.go", "module_test.go"}
			}
			wantLayer(t, status, rows, skips, files)
		})
	}
}

func runInventoryFixture(t *testing.T, root string, files ...string) base.Outcome {
	t.Helper()
	idx := &scip.Index{}
	for _, file := range files {
		idx.Documents = append(idx.Documents, &scip.Document{RelativePath: file})
	}
	encoded, err := proto.Marshal(idx)
	if err != nil {
		t.Fatal(err)
	}
	writeTestFile(t, root, "recorded.scip", string(encoded))
	bin := t.TempDir()
	tool := filepath.Join(bin, "scip-go")
	const stub = `#!/bin/sh
set -eu
case "$1" in
    --version) printf '0.2.10\n' ;;
    index) test "$4" = --output && cp recorded.scip "$5" ;;
    *) exit 1 ;;
esac
`
	if err := os.WriteFile(tool, []byte(stub), 0o700); err != nil {
		t.Fatal(err)
	}
	vars := map[string]string{
		"PATH": bin + string(os.PathListSeparator) + os.Getenv("PATH"), "HOME": t.TempDir(),
		"GOENV": "off", "GOPROXY": "off", "GONOPROXY": "none", "GOSUMDB": "off",
	}
	var stderr strings.Builder
	out, err := base.Run(t.Context(), base.Config{
		Root: root, Out: t.TempDir(), GOOS: runtime.GOOS,
		Caller: adapterenv.Caller{Cwd: root, Vars: vars}, Stderr: &stderr,
	})
	if err != nil {
		t.Fatalf("adapter base: %v, stderr: %s", err, stderr.String())
	}
	t.Logf("base diagnostic: %s", stderr.String())
	return out
}

func applyInventory(t *testing.T, in *Input, out base.Outcome) {
	t.Helper()
	if out.Entry.Status != "ok" {
		t.Fatalf("base failed: %+v", out.Entry)
	}
	in.Env = out.Env
	in.Documents = out.Documents
	in.Inventory = out.Inventory.Owned(in.Root)
	in.Files = map[string]File{}
	for file, read := range out.Reads {
		in.Files[file] = File{Code: read.Code, HasLineDirective: read.LineDirective}
	}
}
