package main

import (
	"bytes"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"testing"

	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/env"
)

// simple writes a module of one package with an index of its files at its root, where the
// stub finds it.
func (w *world) simple(name string, files map[string]string) string {
	w.t.Helper()
	all := map[string]string{"go.mod": "module example.com/" + name + "\n\ngo 1.26\n", "a.go": "package m\n\nfunc A() int { return 1 }\n"}
	for k, v := range files {
		if v == "" {
			delete(all, k)
		} else {
			all[k] = v
		}
	}
	root := w.module(name, all)
	var docs []string
	for rel := range all {
		if strings.HasSuffix(rel, ".go") {
			docs = append(docs, rel)
		}
	}
	writeIndex(w.t, filepath.Join(root, "index.scip"), docs...)
	return root
}

// xmod is a module that requires golang.org/x/mod, with a complete go.sum made offline from
// the test's module cache.
func (w *world) xmod(name string) string {
	w.t.Helper()
	root := w.simple(name, map[string]string{
		"go.mod": "module example.com/" + name + "\n\ngo 1.26\n\nrequire golang.org/x/mod v0.43.0\n",
		"a.go":   "package m\n\nimport \"golang.org/x/mod/semver\"\n\nfunc A() bool { return semver.IsValid(\"v1.0.0\") }\n",
	})
	w.goIn(root, nil, "mod", "download", "golang.org/x/mod")
	w.goIn(root, []string{"GOFLAGS=-mod=mod"}, "list", "./...")
	return root
}

// goIn runs the real go in dir, offline, isolated, with the test's module cache.
func (w *world) goIn(dir string, extra []string, args ...string) {
	w.t.Helper()
	cmd := exec.Command(realGo(w.t), args...)
	cmd.Dir = dir
	cmd.Env = append(isolated([]string{"PATH=/usr/bin:/bin", "GOPROXY=off", "GOSUMDB=off",
		"GOTOOLCHAIN=local", "GOWORK=off", "GOCACHE=" + filepath.Join(w.dir, "test-gocache")}), extra...)
	if out, err := cmd.CombinedOutput(); err != nil {
		w.t.Fatalf("go %v: %v\n%s", args, err, out)
	}
}

func link(t *testing.T, target, path string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(target, path); err != nil {
		t.Fatal(err)
	}
}

type precondition struct {
	name, code string
	setup      func(w *world) (root string, caller env.Caller, goos string)
}

func failing() []precondition {
	native := runtime.GOOS
	return []precondition{
		{"windows", "platform-unsupported", func(w *world) (string, env.Caller, string) {
			w.recordingGo()
			return w.simple("m", nil), w.caller(nil), "windows"
		}},
		{"no go.mod", "manifest-missing", func(w *world) (string, env.Caller, string) {
			return w.simple("m", map[string]string{"go.mod": ""}), w.caller(nil), native
		}},
		{"go exits 1", "toolchain-missing", func(w *world) (string, env.Caller, string) {
			w.fakeGo("exit 1")
			return w.simple("m", nil), w.caller(nil), native
		}},
		{"go env prints no JSON", "toolchain-missing", func(w *world) (string, env.Caller, string) {
			w.fakeGo(`if [ "$1" = env ]; then echo not json; exit 0; fi; exec REALGO "$@"`)
			return w.simple("m", nil), w.caller(nil), native
		}},
		{"go env prints null", "toolchain-missing", func(w *world) (string, env.Caller, string) {
			w.fakeGo(`if [ "$1" = env ]; then echo null; exit 0; fi; exec REALGO "$@"`)
			return w.simple("m", nil), w.caller(nil), native
		}},
		{"go env prints its object and more", "toolchain-missing", func(w *world) (string, env.Caller, string) {
			w.fakeGo(`if [ "$1" = env ]; then REALGO "$@"; echo ']'; exit 0; fi; exec REALGO "$@"`)
			return w.simple("m", nil), w.caller(nil), native
		}},
		{"no scip-go", "toolchain-missing", func(w *world) (string, env.Caller, string) {
			_ = os.Remove(filepath.Join(w.bin, "scip-go"))
			return w.simple("m", nil), w.caller(nil), native
		}},
		{"exported GOMODCACHE in the root, the env file's outside", "tool-home-inside-root", func(w *world) (string, env.Caller, string) {
			root := w.simple("m", nil)
			w.write(filepath.Join(w.home, ".config", "go", "env"), "GOMODCACHE="+filepath.Join(w.dir, "outside")+"\n", 0o644)
			return root, w.caller(map[string]string{"GOMODCACHE": filepath.Join(root, "cache")}), native
		}},
		{"GOMODCACHE in the root", "tool-home-inside-root", func(w *world) (string, env.Caller, string) {
			root := w.simple("m", nil)
			return root, w.caller(map[string]string{"GOMODCACHE": filepath.Join(root, "x", "cache")}), native
		}},
		{"the second GOPATH entry in the root", "tool-home-inside-root", func(w *world) (string, env.Caller, string) {
			root := w.simple("m", nil)
			return root, w.caller(map[string]string{"GOPATH": filepath.Join(w.dir, "gp") + ":" + filepath.Join(root, "gp")}), native
		}},
		{"GOMODCACHE through a link into the root", "tool-home-inside-root", func(w *world) (string, env.Caller, string) {
			root := w.simple("m", nil)
			link(t0(w), root, filepath.Join(w.dir, "to-root"))
			return root, w.caller(map[string]string{"GOMODCACHE": filepath.Join(w.dir, "to-root", "cache")}), native
		}},
		{"GOPATH through a link into the root", "tool-home-inside-root", func(w *world) (string, env.Caller, string) {
			root := w.simple("m", nil)
			link(t0(w), root, filepath.Join(w.dir, "gp-link"))
			return root, w.caller(map[string]string{"GOPATH": filepath.Join(w.dir, "gp-link")}), native
		}},
		{"cache/download a link into the root", "tool-home-inside-root", func(w *world) (string, env.Caller, string) {
			root := w.simple("m", nil)
			cache := filepath.Join(w.dir, "modcache")
			link(t0(w), root, filepath.Join(cache, "cache", "download"))
			return root, w.caller(map[string]string{"GOMODCACHE": cache}), native
		}},
		{"cache/vcs a link into the root", "tool-home-inside-root", func(w *world) (string, env.Caller, string) {
			root := w.simple("m", nil)
			cache := filepath.Join(w.dir, "modcache")
			link(t0(w), root, filepath.Join(cache, "cache", "vcs"))
			return root, w.caller(map[string]string{"GOMODCACHE": cache}), native
		}},
		{"pkg/sumdb of the first GOPATH entry a link into the root", "tool-home-inside-root", func(w *world) (string, env.Caller, string) {
			root := w.simple("m", nil)
			gopath := filepath.Join(w.dir, "gopath")
			link(t0(w), root, filepath.Join(gopath, "pkg", "sumdb"))
			return root, w.caller(map[string]string{"GOPATH": gopath}), native
		}},
		{"GOMODCACHE through a regular file", "tool-home-unresolvable", func(w *world) (string, env.Caller, string) {
			root := w.simple("m", nil)
			w.write(filepath.Join(w.dir, "plain"), "x", 0o644)
			return root, w.caller(map[string]string{"GOMODCACHE": filepath.Join(w.dir, "plain", "cache")}), native
		}},
		{"go 1.99", "toolchain-too-old", func(w *world) (string, env.Caller, string) {
			return w.simple("m", map[string]string{"go.mod": "module example.com/m\n\ngo 1.99\n"}), w.caller(nil), native
		}},
		{"inconsistent vendor/modules.txt", "vendor-inconsistent", func(w *world) (string, env.Caller, string) {
			root := w.xmod("m")
			w.goIn(root, nil, "mod", "vendor")
			modules := filepath.Join(root, "vendor", "modules.txt")
			text, err := os.ReadFile(modules)
			if err != nil {
				t0(w).Fatal(err)
			}
			w.write(modules, strings.Replace(string(text), "## explicit", "", 1), 0o644)
			return root, w.caller(nil), native
		}},
		{"go list -m prints null", "manifest-invalid", func(w *world) (string, env.Caller, string) {
			w.fakeGo(`if [ "$1 $2" = "list -m" ]; then echo null; exit 0; fi; exec REALGO "$@"`)
			return w.simple("m", nil), w.caller(nil), native
		}},
		{"go list -m prints its object and more", "manifest-invalid", func(w *world) (string, env.Caller, string) {
			w.fakeGo(`if [ "$1 $2" = "list -m" ]; then REALGO "$@"; echo ']'; exit 0; fi; exec REALGO "$@"`)
			return w.simple("m", nil), w.caller(nil), native
		}},
		{"go mod edit prints its object and more", "manifest-invalid", func(w *world) (string, env.Caller, string) {
			w.fakeGo(`if [ "$1 $2" = "mod edit" ]; then REALGO "$@"; echo ']'; exit 0; fi; exec REALGO "$@"`)
			return w.simple("m", nil), w.caller(nil), native
		}},
		{"a broken go.mod", "manifest-invalid", func(w *world) (string, env.Caller, string) {
			return w.simple("m", map[string]string{"go.mod": "module example.com/m\n\ngo 1.26\nrequire (\n"}), w.caller(nil), native
		}},
		{"an absolute replace", "replace-absolute", func(w *world) (string, env.Caller, string) {
			dep := w.module("dep", map[string]string{"go.mod": "module example.com/dep\n\ngo 1.26\n", "d.go": "package dep\n\nfunc D() int { return 1 }\n"})
			return w.simple("m", map[string]string{"go.mod": "module example.com/m\n\ngo 1.26\n\nrequire example.com/dep v0.0.0\n\nreplace example.com/dep => \"" + dep + "\"\n"}), w.caller(nil), native
		}},
		{"an absolute replace, vendored, the cache empty", "replace-absolute", func(w *world) (string, env.Caller, string) {
			dep := w.module("dep", map[string]string{"go.mod": "module example.com/dep\n\ngo 1.26\n", "d.go": "package dep\n\nfunc D() int { return 1 }\n"})
			root := w.simple("m", map[string]string{
				"go.mod": "module example.com/m\n\ngo 1.26\n\nrequire example.com/dep v0.0.0\n\nreplace example.com/dep => \"" + dep + "\"\n",
				"a.go":   "package m\n\nimport \"example.com/dep\"\n\nfunc A() int { return dep.D() }\n",
			})
			w.goIn(root, nil, "mod", "vendor")
			return root, w.caller(map[string]string{"GOMODCACHE": filepath.Join(w.dir, "empty-cache")}), native
		}},
		{"the inventory prints no JSON", "dependencies-unreadable", func(w *world) (string, env.Caller, string) {
			w.fakeGo(`for a do if [ "$a" = -deps ]; then echo '{not json'; exit 0; fi; done; exec REALGO "$@"`)
			return w.simple("m", nil), w.caller(nil), native
		}},
		{"a missing package dependency", "dependencies-missing", func(w *world) (string, env.Caller, string) {
			return w.simple("m", map[string]string{"a.go": "package m\n\nimport _ \"example.org/absent/pkg\"\n"}), w.caller(nil), native
		}},
		{"a missing dependency of the external test only", "dependencies-missing", func(w *world) (string, env.Caller, string) {
			return w.simple("m", map[string]string{"a_test.go": "package m_test\n\nimport _ \"example.org/absent/testonly\"\n"}), w.caller(nil), native
		}},
		{"a missing go.sum line", "dependencies-missing", func(w *world) (string, env.Caller, string) {
			root := w.xmod("m")
			sum := filepath.Join(root, "go.sum")
			text, err := os.ReadFile(sum)
			if err != nil {
				t0(w).Fatal(err)
			}
			lines := strings.SplitAfter(string(text), "\n")
			w.write(sum, strings.Join(lines[1:], ""), 0o644)
			return root, w.caller(nil), native
		}},
		{"order: no go.mod before a cache in the root", "manifest-missing", func(w *world) (string, env.Caller, string) {
			root := w.simple("m", map[string]string{"go.mod": ""})
			return root, w.caller(map[string]string{"GOMODCACHE": filepath.Join(root, "cache")}), native
		}},
		{"order: a cache in the root before go 1.99", "tool-home-inside-root", func(w *world) (string, env.Caller, string) {
			root := w.simple("m", map[string]string{"go.mod": "module example.com/m\n\ngo 1.99\n"})
			return root, w.caller(map[string]string{"GOMODCACHE": filepath.Join(root, "cache")}), native
		}},
	}
}

func t0(w *world) *testing.T { return w.t }

func TestPreconditions(t *testing.T) {
	for _, c := range failing() {
		t.Run(c.name, func(t *testing.T) {
			w := newWorld(t)
			root, caller, goos := c.setup(w)
			readOnly(t, root)
			before := listing(t, root)
			var outs []string
			for range 2 {
				out, err := os.MkdirTemp(w.dir, "out-")
				if err != nil {
					t.Fatal(err)
				}
				code, stderr := w.run(caller, goos, "index", "--root", root, "--out", out)
				if code != 0 {
					t.Fatalf("exit %d: %s", code, stderr)
				}
				if !strings.Contains(stderr, c.code) {
					t.Errorf("standard error does not name %s: %q", c.code, stderr)
				}
				outs = append(outs, out)
			}
			entry := baseEntry(t, outs[0])
			if entry.Status != "failed" || len(entry.Diagnostics) != 1 || entry.Diagnostics[0].Code != c.code {
				t.Fatalf("base %+v, want failed with %s alone", entry, c.code)
			}
			if indexRuns(t, w) != 0 {
				t.Error("the indexer ran")
			}
			if c.code == "platform-unsupported" {
				if record := readRecord(t, w.gorec); len(record) != 0 {
					t.Errorf("%d go commands ran before the platform check: %v", len(record), record[0].args)
				}
			}
			layers := result(t, outs[0]).Layers
			for _, layer := range []string{"syntax", "build"} {
				e := layers[layer]
				if e.Status != "failed" || len(e.Diagnostics) != 1 || e.Diagnostics[0].Code != "base-failed" {
					t.Errorf("%s %+v, want failed with base-failed", layer, e)
				}
				if _, err := os.Stat(filepath.Join(outs[0], layer+".jsonl")); !os.IsNotExist(err) {
					t.Errorf("%s.jsonl should be absent for a failed base layer: %v", layer, err)
				}
			}
			if after := listing(t, root); !reflect.DeepEqual(before, after) {
				t.Errorf("the root changed:\n%v\n%v", before, after)
			}
			a, _ := os.ReadFile(filepath.Join(outs[0], "result.json"))
			b, _ := os.ReadFile(filepath.Join(outs[1], "result.json"))
			if !bytes.Equal(a, b) {
				t.Errorf("two runs differ:\n%s\n%s", a, b)
			}
		})
	}
	t.Run("case alias of a missing directory beneath the root", func(t *testing.T) {
		w := newWorld(t)
		root := w.simple("m", nil)
		probe := filepath.Join(w.dir, "case-probe-a")
		if err := os.Mkdir(probe, 0o755); err != nil {
			t.Fatal(err)
		}
		if _, err := os.Stat(filepath.Join(w.dir, "CASE-PROBE-A")); err != nil {
			t.Skip("the temporary volume is case-sensitive: this case needs a case-insensitive volume")
		}
		alias := filepath.Join(filepath.Dir(root), strings.ToUpper(filepath.Base(root)), "new", "cache")
		out, _, stderr := w.index(w.caller(map[string]string{"GOMODCACHE": alias}), root)
		if e := baseEntry(t, out); e.Status != "failed" || e.Diagnostics[0].Code != "tool-home-inside-root" {
			t.Fatalf("base %+v: %s", e, stderr)
		}
	})
}

func TestPreconditionsThatMustNotFail(t *testing.T) {
	t.Run("a broken package imported inside the module", func(t *testing.T) {
		w := newWorld(t)
		root := w.module("m", map[string]string{
			"go.mod":  "module example.com/m\n\ngo 1.26\n",
			"main.go": "package main\n\nimport \"example.com/m/a\"\n\nfunc main() { a.A() }\n",
			"a/a.go":  "package a\n\nimport \"example.com/m/b\"\n\nfunc A() { b.B() }\n",
			"b/b.go":  "packag b\n",
		})
		writeIndex(t, filepath.Join(root, "index.scip"), "main.go", "a/a.go")
		out, code, stderr := w.index(w.caller(nil), root)
		entry := baseEntry(t, out)
		if code != 0 || entry.Status != "ok" || indexRuns(t, w) != 1 {
			t.Fatalf("exit %d, base %+v: %s", code, entry, stderr)
		}
		if !reflect.DeepEqual(entry.Skipped, []contract.Skipped{{File: "b/b.go", Code: "parse-failed"}}) {
			t.Fatalf("skipped %+v", entry.Skipped)
		}
	})
	t.Run("an import from an ignored directory", func(t *testing.T) {
		w := newWorld(t)
		root := w.module("m", map[string]string{
			"go.mod":         "module example.com/m\n\ngo 1.26\n\nignore ./ign\n",
			"app/app.go":     "package app\n\nimport \"example.com/m/ign/lib\"\n\nfunc A() int { return lib.L() }\n",
			"ign/lib/lib.go": "package lib\n\nfunc L() int { return 1 }\n",
		})
		writeIndex(t, filepath.Join(root, "index.scip"), "app/app.go")
		out, code, stderr := w.index(w.caller(nil), root)
		entry := baseEntry(t, out)
		if code != 0 || entry.Status != "ok" || len(entry.Skipped) != 0 {
			t.Fatalf("exit %d, base %+v: %s", code, entry, stderr)
		}
	})
}

// Each of the eight settings, exported, wins over a conflicting value in the env file, and the
// later invocations carry the value the bootstrap returned.
func TestExportedSettings(t *testing.T) {
	for _, name := range env.Settings {
		t.Run(name, func(t *testing.T) {
			w := newWorld(t)
			w.recordingGo()
			root := w.simple("m", nil)
			fromFile, exported := "file.example.invalid", "exported.example.invalid"
			if name == "GOMODCACHE" || name == "GOPATH" {
				fromFile, exported = filepath.Join(w.dir, "from-file"), filepath.Join(w.dir, "exported")
			}
			if name == "GOPROXY" {
				fromFile, exported = "https://"+fromFile, "https://"+exported
			}
			w.write(filepath.Join(w.home, ".config", "go", "env"), name+"="+fromFile+"\n", 0o644)
			out, code, stderr := w.index(w.caller(map[string]string{name: exported}), root)
			if code != 0 || baseEntry(t, out).Status != "ok" {
				t.Fatalf("exit %d, base %+v: %s", code, baseEntry(t, out), stderr)
			}
			record := readRecord(t, w.gorec)
			if len(record) < 2 || record[0].args[0] != "env" {
				t.Fatalf("record %+v", record)
			}
			if got := record[0].vars[name]; got != exported {
				t.Errorf("the bootstrap got %s=%q", name, got)
			}
			for _, inv := range record[1:] {
				if got := inv.vars[name]; got != exported {
					t.Errorf("%v got %s=%q", inv.args, name, got)
				}
			}
		})
	}
}
