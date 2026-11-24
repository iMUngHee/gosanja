package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"testing"

	"github.com/dlclark/regexp2"
	"github.com/santhosh-tekuri/jsonschema/v6"
	"github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"

	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/env"
)

// repo is the repository root, two directories above the module root.
func repo(t *testing.T) string {
	t.Helper()
	dir, err := filepath.Abs("../../../..")
	if err != nil {
		t.Fatal(err)
	}
	return dir
}

// realGo is the go command the tests run under, found on the test's own PATH.
func realGo(t *testing.T) string {
	t.Helper()
	path, err := exec.LookPath("go")
	if err != nil {
		t.Fatalf("the tests need the go command: %v", err)
	}
	return path
}

// The go commands a test runs itself, outside the adapter, see a home of their own with Go
// telemetry off (directHome), so that none writes into the caller's home, and none leaves a
// detached telemetry process writing in a directory a test is about to remove. They keep the
// caller's module cache and GOPATH (cache, gopath), read once from the caller's own settings.
var directHome, cache, gopath string

// isolate prepares directHome under scratch and reads the caller's module cache and GOPATH.
// The query reads the caller's env file but writes its telemetry under directHome: on Linux
// Go takes its configuration directory from XDG_CONFIG_HOME.
func isolate(scratch string) error {
	directHome = filepath.Join(scratch, "direct-home")
	mode := filepath.Join(directHome, ".config", "go", "telemetry", "mode")
	if err := os.MkdirAll(filepath.Dir(mode), 0o755); err != nil {
		return err
	}
	if err := os.WriteFile(mode, []byte("off\n"), 0o644); err != nil {
		return err
	}
	config, err := os.UserConfigDir()
	if err != nil {
		return err
	}
	envFile := os.Getenv("GOENV")
	if envFile == "" {
		envFile = filepath.Join(config, "go", "env")
	}
	cmd := exec.Command("go", "env", "GOMODCACHE", "GOPATH")
	cmd.Env = append(os.Environ(), "GOENV="+envFile, "XDG_CONFIG_HOME="+filepath.Join(directHome, ".config"))
	out, err := cmd.Output()
	if err != nil {
		return fmt.Errorf("go env GOMODCACHE GOPATH: %w", err)
	}
	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	if len(lines) != 2 {
		return fmt.Errorf("go env GOMODCACHE GOPATH printed %q", out)
	}
	cache, gopath = lines[0], lines[1]
	return nil
}

// isolated is env with the isolated home and the caller's module cache and GOPATH, which
// replace any earlier value (exec keeps the last of a duplicated name).
func isolated(env []string) []string {
	return append(env, "HOME="+directHome, "XDG_CONFIG_HOME="+filepath.Join(directHome, ".config"),
		"GOMODCACHE="+cache, "GOPATH="+gopath)
}

// moduleCache is the caller's module cache, which holds the adapter's build list and so
// golang.org/x/mod, which the fixture requires.
func moduleCache(t *testing.T) string {
	t.Helper()
	if cache == "" {
		t.Fatal("TestMain did not read the module cache")
	}
	return cache
}

// world is one test's temporary directories: a bin directory first on PATH, a caller home, and
// the tools the adapter finds there.
type world struct {
	t      *testing.T
	dir    string
	bin    string
	home   string
	record string // the stub scip-go's record
	gorec  string // the recording go's record, when installed
}

func newWorld(t *testing.T) *world {
	t.Helper()
	dir := t.TempDir()
	w := &world{t: t, dir: dir, bin: filepath.Join(dir, "bin"), home: filepath.Join(dir, "home"),
		record: filepath.Join(dir, "scip-go.record")}
	for _, d := range []string{w.bin, w.home} {
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.Symlink(realGo(t), filepath.Join(w.bin, "go")); err != nil {
		t.Fatal(err)
	}
	w.stub("")
	return w
}

// stub installs a copy of fixtures/stub-tools/scip-go that records into w.record and copies
// index, or the root's index.scip when index is empty.
func (w *world) stub(index string) {
	w.t.Helper()
	src, err := os.ReadFile(filepath.Join(repo(w.t), "fixtures", "stub-tools", "scip-go"))
	if err != nil {
		w.t.Fatal(err)
	}
	text := strings.Replace(string(src), "index_file=''", "index_file='"+index+"'", 1)
	text = strings.Replace(text, "record_file=''", "record_file='"+w.record+"'", 1)
	w.write(filepath.Join(w.bin, "scip-go"), text, 0o755)
}

// recorderSource is a program that logs its arguments, working directory and exact
// environment to the file named on the first line of <itself>.conf, then executes the program
// named on the second. A shell script cannot stand in for it: the shell adds PWD by itself.
const recorderSource = `package main

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
	"syscall"
)

func main() {
	self, _ := os.Executable()
	conf, err := os.ReadFile(self + ".conf")
	if err != nil {
		os.Exit(125)
	}
	lines := strings.Split(strings.TrimSpace(string(conf)), "\n")
	cwd, _ := os.Getwd()
	cwd, _ = filepath.EvalSymlinks(cwd)
	var b strings.Builder
	b.WriteString("cwd=" + cwd + "\n")
	for _, a := range os.Args[1:] {
		b.WriteString("arg=" + a + "\n")
	}
	vars := os.Environ()
	sort.Strings(vars)
	for _, v := range vars {
		b.WriteString(v + "\n")
	}
	b.WriteString("\n")
	f, err := os.OpenFile(lines[0], os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		os.Exit(125)
	}
	f.WriteString(b.String())
	f.Close()
	os.Exit(func() int { syscall.Exec(lines[1], append([]string{lines[1]}, os.Args[1:]...), os.Environ()); return 126 }())
}
`

// recordingGo replaces go with a recorder that logs each invocation's arguments, working
// directory and exact environment, then runs the real go.
func (w *world) recordingGo() {
	w.t.Helper()
	w.gorec = filepath.Join(w.dir, "go.record")
	src := filepath.Join(w.dir, "recorder", "main.go")
	w.write(src, recorderSource, 0o644)
	path := filepath.Join(w.bin, "go")
	_ = os.Remove(path)
	cmd := exec.Command(realGo(w.t), "build", "-o", path, src)
	cmd.Dir = filepath.Dir(src)
	cmd.Env = isolated(append(os.Environ(), "GOTOOLCHAIN=local", "GOWORK=off", "GOFLAGS=", "CGO_ENABLED=0"))
	if out, err := cmd.CombinedOutput(); err != nil {
		w.t.Fatalf("build the recorder: %v\n%s", err, out)
	}
	w.write(path+".conf", w.gorec+"\n"+realGo(w.t)+"\n", 0o644)
}

// fakeGo replaces go with a script.
func (w *world) fakeGo(script string) {
	w.t.Helper()
	path := filepath.Join(w.bin, "go")
	_ = os.Remove(path)
	w.write(path, "#!/bin/sh\n"+strings.ReplaceAll(script, "REALGO", realGo(w.t))+"\n", 0o755)
}

func (w *world) write(path, text string, mode fs.FileMode) {
	w.t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		w.t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(text), mode); err != nil {
		w.t.Fatal(err)
	}
}

// caller is the environment the adapter is started in: PATH with the world's tools first, the
// caller's home, an offline proxy and the test's module cache.
func (w *world) caller(extra map[string]string) env.Caller {
	vars := map[string]string{
		"PATH":       w.bin + ":/usr/bin:/bin",
		"HOME":       w.home,
		"GOPROXY":    "off",
		"GOMODCACHE": moduleCache(w.t),
	}
	for k, v := range extra {
		if v == "" {
			delete(vars, k)
		} else {
			vars[k] = v
		}
	}
	home := vars["HOME"]
	return env.Caller{Vars: vars, Cwd: w.dir, ConfigDir: func() (string, error) {
		if x := vars["XDG_CONFIG_HOME"]; x != "" {
			return x, nil
		}
		return filepath.Join(home, ".config"), nil
	}}
}

// module writes a module under w.dir/name from file contents and returns its root.
func (w *world) module(name string, files map[string]string) string {
	w.t.Helper()
	root := filepath.Join(w.dir, name)
	for rel, text := range files {
		w.write(filepath.Join(root, rel), text, 0o644)
	}
	return root
}

// copyTree copies a directory, links as links.
func copyTree(t *testing.T, from, to string) {
	t.Helper()
	err := filepath.WalkDir(from, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(from, path)
		target := filepath.Join(to, rel)
		info, err := os.Lstat(path)
		if err != nil {
			return err
		}
		switch {
		case info.Mode()&fs.ModeSymlink != 0:
			link, err := os.Readlink(path)
			if err != nil {
				return err
			}
			return os.Symlink(link, target)
		case d.IsDir():
			return os.MkdirAll(target, 0o755)
		default:
			data, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			return os.WriteFile(target, data, info.Mode().Perm())
		}
	})
	if err != nil {
		t.Fatal(err)
	}
}

// flow copies fixtures/go-flow into the world under name.
func (w *world) flow(name string) string {
	w.t.Helper()
	root := filepath.Join(w.dir, name)
	copyTree(w.t, filepath.Join(repo(w.t), "fixtures", "go-flow"), root)
	return root
}

// listing is every entry under dir, hidden ones included, with its mode and, for a file, the
// digest of its content.
func listing(t *testing.T, dir string) []string {
	t.Helper()
	var out []string
	err := filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		info, err := os.Lstat(path)
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(dir, path)
		entry := rel + " " + info.Mode().String()
		if info.Mode().IsRegular() {
			data, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			sum := sha256.Sum256(data)
			entry += " " + hex.EncodeToString(sum[:])
		}
		out = append(out, entry)
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	sort.Strings(out)
	return out
}

// readOnly removes write permission from every entry under dir, and restores it at cleanup.
func readOnly(t *testing.T, dir string) {
	t.Helper()
	var dirs []string
	_ = filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err == nil && d.Type()&fs.ModeSymlink == 0 {
			info, _ := os.Lstat(path)
			_ = os.Chmod(path, info.Mode().Perm()&^0o222)
			if d.IsDir() {
				dirs = append(dirs, path)
			}
		}
		return nil
	})
	t.Cleanup(func() {
		for _, d := range dirs {
			_ = os.Chmod(d, 0o755)
		}
	})
}

// run starts the adapter in process and returns its exit status and standard error.
// run runs the command in process. go/packages finds go through the process's own PATH, as
// it does when the caller starts the adapter, so the process takes the caller's PATH here.
func (w *world) run(caller env.Caller, goos string, args ...string) (int, string) {
	w.t.Setenv("PATH", caller.Vars["PATH"])
	var stdout, stderr bytes.Buffer
	code := run(args, caller, goos, &stdout, &stderr)
	return code, stderr.String()
}

// index runs `index --root root --out <fresh dir>` and returns the output directory.
func (w *world) index(caller env.Caller, root string, extra ...string) (string, int, string) {
	w.t.Helper()
	out, err := os.MkdirTemp(w.dir, "out-")
	if err != nil {
		w.t.Fatal(err)
	}
	code, stderr := w.run(caller, runtime.GOOS, append([]string{"index", "--root", root, "--out", out}, extra...)...)
	return out, code, stderr
}

// result reads and schema-checks result.json.
func result(t *testing.T, out string) contract.Result {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(out, "result.json"))
	if err != nil {
		t.Fatalf("result.json: %v", err)
	}
	validate(t, "adapter-result.schema.json", data)
	var r contract.Result
	if err := json.Unmarshal(data, &r); err != nil {
		t.Fatal(err)
	}
	return r
}

// validate checks a JSON document against a schema of spec/schema.
func validate(t *testing.T, schema string, data []byte) {
	t.Helper()
	dir := filepath.Join(repo(t), "spec", "schema")
	c := jsonschema.NewCompiler()
	// The schemas' patterns use lookahead, which Go's regexp does not have; JSON Schema's
	// patterns are ECMA-262 regular expressions.
	c.UseRegexpEngine(ecmaRegexp)
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		raw, err := os.ReadFile(filepath.Join(dir, e.Name()))
		if err != nil {
			t.Fatal(err)
		}
		doc, err := jsonschema.UnmarshalJSON(bytes.NewReader(raw))
		if err != nil {
			t.Fatal(err)
		}
		if err := c.AddResource(e.Name(), doc); err != nil {
			t.Fatal(err)
		}
	}
	s, err := c.Compile(schema)
	if err != nil {
		t.Fatal(err)
	}
	value, err := jsonschema.UnmarshalJSON(bytes.NewReader(data))
	if err != nil {
		t.Fatal(err)
	}
	if err := s.Validate(value); err != nil {
		t.Fatalf("%s does not pass %s: %v", data, schema, err)
	}
}

// writeIndex writes an index with one document per path.
func writeIndex(t *testing.T, path string, documents ...string) {
	t.Helper()
	var idx scip.Index
	idx.Metadata = &scip.Metadata{ToolInfo: &scip.ToolInfo{Name: "scip-go", Version: "0.2.10"}}
	for _, d := range documents {
		idx.Documents = append(idx.Documents, &scip.Document{RelativePath: d, Language: "go"})
	}
	data, err := proto.Marshal(&idx)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatal(err)
	}
}

// invocations reads a record as one map of variables per invocation, with `cwd` and the
// arguments under `arg`.
type invocation struct {
	cwd  string
	args []string
	vars map[string]string
}

func readRecord(t *testing.T, path string) []invocation {
	t.Helper()
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		t.Fatal(err)
	}
	var out []invocation
	for _, block := range strings.Split(strings.TrimSpace(string(data)), "\n\n") {
		inv := invocation{vars: map[string]string{}}
		for _, line := range strings.Split(block, "\n") {
			k, v, _ := strings.Cut(line, "=")
			switch k {
			case "cwd":
				inv.cwd = v
			case "arg":
				inv.args = append(inv.args, v)
			default:
				inv.vars[k] = v
			}
		}
		out = append(out, inv)
	}
	return out
}

// indexRuns counts the stub's `index` invocations.
func indexRuns(t *testing.T, w *world) int {
	n := 0
	for _, inv := range readRecord(t, w.record) {
		if len(inv.args) > 0 && inv.args[0] == "index" {
			n++
		}
	}
	return n
}

// base returns the base layer's entry of a result.
func baseEntry(t *testing.T, out string) contract.Entry {
	t.Helper()
	r := result(t, out)
	entry, ok := r.Layers["base"]
	if !ok {
		t.Fatalf("no base entry in %+v", r)
	}
	return entry
}

type ecma regexp2.Regexp

func (re *ecma) MatchString(s string) bool {
	matched, err := (*regexp2.Regexp)(re).MatchString(s)
	return err == nil && matched
}

func (re *ecma) String() string { return (*regexp2.Regexp)(re).String() }

func ecmaRegexp(s string) (jsonschema.Regexp, error) {
	re, err := regexp2.Compile(s, regexp2.ECMAScript)
	if err != nil {
		return nil, err
	}
	return (*ecma)(re), nil
}
