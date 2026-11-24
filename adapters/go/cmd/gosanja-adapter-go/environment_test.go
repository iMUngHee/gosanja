package main

import (
	"bytes"
	"io/fs"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"

	"gosanja/adapters/go/internal/env"
)

// stubAdded are the variables the stub's shell sets by itself, with any value.
var stubAdded = map[string]string{"PWD": "", "SHLVL": "", "_": ""}

// releaseTagProbe is the one go command go/packages runs with GO111MODULE=off.
var releaseTagProbe = []string{"list", "-e", "-f", "{{context.ReleaseTags}}", "--", "unsafe"}

// own reports whether a logged go invocation is one the adapter starts itself rather than one
// go/packages starts for the syntax layer.
func own(args []string) bool {
	line := strings.Join(args, " ")
	return line == "version" || line == "list -m -json" || line == "mod edit -json" ||
		strings.HasPrefix(line, "list -e -test -deps -json=ImportPath,")
}

// goAdded are the variables a logged go invocation may carry beyond the child environment:
// none for the adapter's own, PWD set to the working directory for go/packages', and
// GO111MODULE=off as well for its release-tag probe.
func goAdded(inv invocation, root string) map[string]string {
	switch {
	case own(inv.args):
		return nil
	case reflect.DeepEqual(inv.args, releaseTagProbe):
		return map[string]string{"PWD": root, "GO111MODULE": "off"}
	default:
		return map[string]string{"PWD": root}
	}
}

// checkGo checks every logged go invocation after the first, and that go/packages ran, with its
// release-tag probe among its commands.
func checkGo(t *testing.T, record []invocation, root, out string, vendor bool) {
	t.Helper()
	loader, probe := 0, false
	for _, inv := range record {
		checkChild(t, inv, root, out, vendor, goAdded(inv, root))
		if !own(inv.args) {
			loader++
			probe = probe || reflect.DeepEqual(inv.args, releaseTagProbe)
		}
	}
	if loader == 0 || !probe {
		t.Errorf("go/packages ran %d go commands, the release-tag probe among them: %v; logged %d", loader, probe, len(record))
	}
}

// passedNames are the caller variables a child may carry unchanged.
var passedNames = map[string]bool{"PATH": true, "HTTP_PROXY": true, "HTTPS_PROXY": true, "ALL_PROXY": true, "NO_PROXY": true,
	"http_proxy": true, "https_proxy": true, "all_proxy": true, "no_proxy": true, "SSL_CERT_FILE": true, "SSL_CERT_DIR": true}

func settingName(k string) bool {
	for _, s := range env.Settings {
		if s == k {
			return true
		}
	}
	return false
}

// checkBootstrap checks the first logged invocation: the query for the eight names, in
// <out>/work, with the env file, every write destination under <out>/work and nothing else
// but the caller's PATH, network variables and exported settings.
func checkBootstrap(t *testing.T, inv invocation, out, envFile string) {
	t.Helper()
	work := filepath.Join(out, "work")
	if want := append([]string{"env", "-json"}, env.Settings...); !reflect.DeepEqual(inv.args, want) {
		t.Errorf("first invocation %v", inv.args)
	}
	if inv.cwd != work {
		t.Errorf("first invocation in %s", inv.cwd)
	}
	want := map[string]string{
		"GOENV": envFile, "HOME": filepath.Join(work, "home"), "XDG_CONFIG_HOME": filepath.Join(work, "config"),
		"GOCACHE": filepath.Join(work, "cache"), "TMPDIR": filepath.Join(work, "tmp"), "GOTMPDIR": filepath.Join(work, "tmp"),
		"GOTOOLCHAIN": "local", "GOWORK": "off", "GOPACKAGESDRIVER": "off",
	}
	for k, v := range want {
		if inv.vars[k] != v {
			t.Errorf("bootstrap %s=%q, want %q", k, inv.vars[k], v)
		}
	}
	for k := range inv.vars {
		if _, ok := want[k]; !ok && !passedNames[k] && !settingName(k) {
			t.Errorf("bootstrap carries %s", k)
		}
	}
}

// checkChild checks a later invocation: the root as working directory and the child
// environment, with nothing else but the added variables (an empty value admits any).
func checkChild(t *testing.T, inv invocation, root, out string, vendor bool, added map[string]string) {
	t.Helper()
	work := filepath.Join(out, "work")
	if inv.cwd != root {
		t.Errorf("%v in %s", inv.args, inv.cwd)
	}
	mod := "-mod=readonly"
	if vendor {
		mod = "-mod=vendor"
	}
	want := map[string]string{
		"GOENV": "off", "HOME": filepath.Join(work, "home"), "XDG_CONFIG_HOME": filepath.Join(work, "config"),
		"GOCACHE": filepath.Join(work, "cache"), "TMPDIR": filepath.Join(work, "tmp"), "GOTMPDIR": filepath.Join(work, "tmp"),
		"GOTOOLCHAIN": "local", "GOWORK": "off", "GOPACKAGESDRIVER": "off", "CGO_ENABLED": "0",
		"GOFLAGS": "-buildvcs=false " + mod,
	}
	for k, v := range want {
		if inv.vars[k] != v {
			t.Errorf("%v: %s=%q, want %q", inv.args, k, inv.vars[k], v)
		}
	}
	for k, v := range inv.vars {
		extra, ok := added[k]
		switch {
		case ok && extra != "" && v != extra:
			t.Errorf("%v: %s=%q, want %q", inv.args, k, v, extra)
		case ok:
		case want[k] == "" && !passedNames[k] && !settingName(k):
			t.Errorf("%v carries %s", inv.args, k)
		}
	}
}

func TestEnvironment(t *testing.T) {
	type setup struct {
		extra   map[string]string
		prepare func(w *world, root string)
		envFile func(w *world) string // the GOENV the bootstrap must get
		proxy   func(w *world) string // the GOPROXY the later invocations must carry
	}
	// fileProxy serves the test's module cache, so that a GOPROXY from the env file works offline.
	fileProxy := func(w *world) string { return "file://" + filepath.Join(moduleCache(w.t), "cache", "download") }
	defaultFile := func(w *world) string { return filepath.Join(w.home, ".config", "go", "env") }
	cases := map[string]setup{
		"read-only": {prepare: func(w *world, root string) { readOnly(w.t, root) }},
		"a caller's environment that points into the root": {
			extra: map[string]string{"GOFLAGS": "-mod=mod", "GOWORK": "ROOT/go.work", "GOTOOLCHAIN": "auto",
				"GOCACHE": "ROOT/cache", "TMPDIR": "ROOT/tmp", "GOPACKAGESDRIVER": "ROOT/driver"},
		},
		"the user's env file sets GOFLAGS and GOPROXY": {
			extra: map[string]string{"GOPROXY": ""},
			prepare: func(w *world, root string) {
				w.write(defaultFile(w), "GOFLAGS=-mod=mod\nGOPROXY="+fileProxy(w)+"\n", 0o644)
			},
			proxy: fileProxy,
		},
		"a packages driver on PATH": {
			prepare: func(w *world, root string) {
				w.write(filepath.Join(w.bin, "gopackagesdriver"), "#!/bin/sh\necho called >> '"+filepath.Join(w.dir, "driver.record")+"'\nexit 1\n", 0o755)
			},
		},
		"a relative GOENV": {
			extra: map[string]string{"GOENV": "conf/env"},
			prepare: func(w *world, root string) {
				w.write(filepath.Join(w.dir, "conf", "env"), "GONOSUMDB=relative.example.invalid\n", 0o644)
			},
			envFile: func(w *world) string { return filepath.Join(w.dir, "conf", "env") },
		},
		"GOENV off": {
			extra:   map[string]string{"GOENV": "off"},
			envFile: func(*world) string { return "off" },
		},
	}
	for name, c := range cases {
		t.Run(name, func(t *testing.T) {
			w := newWorld(t)
			w.recordingGo()
			root := w.flow("go-flow")
			extra := map[string]string{}
			for k, v := range c.extra {
				extra[k] = strings.ReplaceAll(v, "ROOT", root)
			}
			if c.prepare != nil {
				c.prepare(w, root)
			}
			before := listing(t, root)
			configBefore := listing(t, w.home)
			out, code, stderr := w.index(w.caller(extra), root)
			if code != 0 || baseEntry(t, out).Status != "ok" {
				t.Fatalf("exit %d, base %+v: %s", code, baseEntry(t, out), stderr)
			}
			if after := listing(t, root); !reflect.DeepEqual(before, after) {
				t.Errorf("the root changed")
			}
			if after := listing(t, w.home); !reflect.DeepEqual(configBefore, after) {
				t.Errorf("the caller's home changed")
			}
			if _, err := os.Stat(filepath.Join(w.dir, "driver.record")); err == nil {
				t.Errorf("the packages driver ran")
			}
			envFile := defaultFile(w)
			if c.envFile != nil {
				envFile = c.envFile(w)
			}
			record := readRecord(t, w.gorec)
			if len(record) < 2 {
				t.Fatalf("record %+v", record)
			}
			checkBootstrap(t, record[0], out, envFile)
			proxy := "off"
			if c.proxy != nil {
				proxy = c.proxy(w)
			}
			checkTelemetryOff(t, out)
			checkGo(t, record[1:], root, out, false)
			for _, inv := range readRecord(t, w.record) {
				checkChild(t, inv, root, out, false, stubAdded)
			}
			for _, inv := range append(record[1:], readRecord(t, w.record)...) {
				if inv.vars["GOPROXY"] != proxy {
					t.Errorf("%v: GOPROXY=%q, want %q", inv.args, inv.vars["GOPROXY"], proxy)
				}
			}
		})
	}
}

func TestEnvironmentDefaults(t *testing.T) {
	w := newWorld(t)
	w.recordingGo()
	root := w.simple("m", nil)
	out, code, stderr := w.index(w.caller(map[string]string{"GOMODCACHE": "", "GOPATH": ""}), root)
	if code != 0 || baseEntry(t, out).Status != "ok" {
		t.Fatalf("exit %d, base %+v: %s", code, baseEntry(t, out), stderr)
	}
	for _, inv := range readRecord(t, w.gorec)[1:] {
		if inv.vars["GOPATH"] != filepath.Join(w.home, "go") || inv.vars["GOMODCACHE"] != filepath.Join(w.home, "go", "pkg", "mod") {
			t.Errorf("%v: GOPATH=%q GOMODCACHE=%q", inv.args, inv.vars["GOPATH"], inv.vars["GOMODCACHE"])
		}
	}
}

func TestEnvironmentTelemetry(t *testing.T) {
	for _, which := range []string{"config directory", "telemetry directory"} {
		t.Run(which, func(t *testing.T) {
			w := newWorld(t)
			root := w.flow("go-flow")
			into := filepath.Join(root, "inside")
			if err := os.Mkdir(into, 0o755); err != nil {
				t.Fatal(err)
			}
			config := filepath.Join(w.dir, "config")
			if which == "config directory" {
				link(t, into, config)
			} else {
				link(t, into, filepath.Join(config, "go", "telemetry", "local"))
			}
			out, code, stderr := w.index(w.caller(map[string]string{"XDG_CONFIG_HOME": config}), root)
			if code != 0 || baseEntry(t, out).Status != "ok" {
				t.Fatalf("exit %d: %s", code, stderr)
			}
			if entries, _ := os.ReadDir(into); len(entries) != 0 {
				t.Errorf("%d files appeared in the root", len(entries))
			}
			checkTelemetryOff(t, out)
		})
	}
}

// checkTelemetryOff checks that the adapter's Go processes kept telemetry off: the mode file
// it wrote, no counters or upload directory under <out>/work, and no process left that runs
// with that configuration directory.
func checkTelemetryOff(t *testing.T, out string) {
	t.Helper()
	telemetry := filepath.Join(out, "work", "config", "go", "telemetry")
	if mode, err := os.ReadFile(filepath.Join(telemetry, "mode")); err != nil || strings.TrimSpace(string(mode)) != "off" {
		t.Errorf("telemetry mode %q: %v", mode, err)
	}
	for _, dir := range []string{"local", "upload"} {
		if _, err := os.Lstat(filepath.Join(telemetry, dir)); err == nil {
			t.Errorf("telemetry wrote %s under <out>/work/config", dir)
		}
	}
	if pids := lingering(t, "XDG_CONFIG_HOME="+filepath.Join(out, "work", "config")); len(pids) != 0 {
		t.Errorf("processes still run with the adapter's configuration directory: %v", pids)
	}
}

// lingering lists the processes of this user whose environment holds the variable, as /proc
// shows them on Linux.
func lingering(t *testing.T, variable string) []string {
	t.Helper()
	entries, err := os.ReadDir("/proc")
	if err != nil {
		t.Fatalf("this check reads /proc: %v", err)
	}
	var pids []string
	for _, e := range entries {
		environ, err := os.ReadFile(filepath.Join("/proc", e.Name(), "environ"))
		if err != nil {
			continue
		}
		for _, v := range bytes.Split(environ, []byte{0}) {
			if string(v) == variable {
				pids = append(pids, e.Name())
			}
		}
	}
	return pids
}

func TestEnvironmentGoSum(t *testing.T) {
	w := newWorld(t)
	root := w.flow("go-flow")
	sum := filepath.Join(root, "go.sum")
	text, err := os.ReadFile(sum)
	if err != nil {
		t.Fatal(err)
	}
	var kept []string
	removed := ""
	for _, line := range strings.SplitAfter(string(text), "\n") {
		if strings.Contains(line, "/go.mod h1:") {
			removed = line
		} else {
			kept = append(kept, line)
		}
	}
	cut := []byte(strings.Join(kept, ""))
	if bytes.Equal(cut, text) {
		t.Fatal("the fixture's go.sum has no go.mod line")
	}
	w.write(sum, string(cut), 0o644)
	out, code, stderr := w.index(w.caller(map[string]string{"GOFLAGS": "-mod=mod"}), root)
	if e := baseEntry(t, out); code != 0 || e.Status != "failed" || e.Diagnostics[0].Code != "dependencies-missing" {
		t.Fatalf("exit %d, base %+v: %s", code, e, stderr)
	}
	if after, _ := os.ReadFile(sum); !bytes.Equal(after, cut) {
		t.Errorf("go.sum was completed")
	}
	w.goIn(root, []string{"GOFLAGS=-mod=mod"}, "list", "-m", "all")
	if after, _ := os.ReadFile(sum); !strings.Contains(string(after), removed) {
		t.Errorf("control: go list -m all under -mod=mod did not restore %q:\n%s", removed, after)
	}
}

type listener struct {
	*httptest.Server
	mu   sync.Mutex
	hits []string
}

// newListener records each request; with a module cache it serves the cache's download
// directory, which has the layout of a module proxy, and otherwise answers 404.
func newListener(t *testing.T, cache string) *listener {
	l := &listener{}
	files := http.NotFoundHandler()
	if cache != "" {
		files = http.FileServer(http.Dir(filepath.Join(cache, "cache", "download")))
	}
	l.Server = httptest.NewServer(http.HandlerFunc(func(rw http.ResponseWriter, r *http.Request) {
		l.mu.Lock()
		l.hits = append(l.hits, r.URL.Path)
		l.mu.Unlock()
		files.ServeHTTP(rw, r)
	}))
	t.Cleanup(l.Close)
	return l
}

func (l *listener) count() int {
	l.mu.Lock()
	defer l.mu.Unlock()
	return len(l.hits)
}

func TestEnvironmentProxy(t *testing.T) {
	w := newWorld(t)
	w.recordingGo()
	exported, fromFile := newListener(t, moduleCache(t)), newListener(t, "")
	root := w.flow("go-flow")
	w.write(filepath.Join(w.home, ".config", "go", "env"), "GOPROXY="+fromFile.URL+"\n", 0o644)
	cold := filepath.Join(w.dir, "cold-cache")
	// The go command leaves the module cache read-only; the temporary directory's removal needs it writable.
	t.Cleanup(func() {
		_ = filepath.WalkDir(cold, func(path string, d fs.DirEntry, err error) error {
			if err == nil && d.IsDir() {
				_ = os.Chmod(path, 0o755)
			}
			return nil
		})
	})
	out, code, stderr := w.index(w.caller(map[string]string{"GOPROXY": exported.URL, "GOMODCACHE": cold}), root)
	if code != 0 || baseEntry(t, out).Status != "ok" {
		t.Fatalf("exit %d, base %+v: %s", code, baseEntry(t, out), stderr)
	}
	if exported.count() == 0 || fromFile.count() != 0 {
		t.Fatalf("requests: exported %d, env file %d", exported.count(), fromFile.count())
	}
	for _, inv := range readRecord(t, w.gorec)[1:] {
		if inv.vars["GOPROXY"] != exported.URL {
			t.Errorf("%v: GOPROXY=%q", inv.args, inv.vars["GOPROXY"])
		}
	}
}

func TestEnvironmentGit(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Fatal("this test needs git")
	}
	w := newWorld(t)
	root := w.flow("go-flow")
	gitRecord := filepath.Join(w.dir, "git.record")
	realGit, _ := exec.LookPath("git")
	w.write(filepath.Join(w.bin, "git"), "#!/bin/sh\necho \"$*\" >> '"+gitRecord+"'\nexec '"+realGit+"' \"$@\"\n", 0o755)
	gitIn := func(args ...string) {
		cmd := exec.Command(realGit, args...)
		cmd.Dir = root
		cmd.Env = []string{"PATH=/usr/bin:/bin", "HOME=" + w.home, "GIT_AUTHOR_NAME=p", "GIT_AUTHOR_EMAIL=p@example.invalid",
			"GIT_COMMITTER_NAME=p", "GIT_COMMITTER_EMAIL=p@example.invalid"}
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	gitIn("init", "-q")
	gitIn("add", "-A")
	gitIn("commit", "-qm", "fixture")
	index := filepath.Join(root, ".git", "index")
	stale := func() []byte {
		later := filepath.Join(root, "flow.go")
		info, _ := os.Stat(later)
		if err := os.Chtimes(later, info.ModTime().Add(2e9), info.ModTime().Add(2e9)); err != nil {
			t.Fatal(err)
		}
		data, _ := os.ReadFile(index)
		return data
	}
	// Positive controls: default stamping runs git status, and git status rewrites a stale index.
	stale()
	cmd := exec.Command(realGo(t), "list", "-e", "-test", "-deps", "-json", "./...")
	cmd.Dir = root
	cmd.Env = isolated([]string{"PATH=" + w.bin + ":/usr/bin:/bin", "GOPROXY=off", "GOTOOLCHAIN=local", "GOWORK=off",
		"GOCACHE=" + filepath.Join(w.dir, "control-cache"), "CGO_ENABLED=0"})
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("control go list: %v\n%s", err, out)
	}
	if log, _ := os.ReadFile(gitRecord); !strings.Contains(string(log), "status --porcelain") {
		t.Fatalf("control: go list did not run git status; git ran %q", log)
	}
	before := stale()
	gitIn("status", "--porcelain")
	if after, _ := os.ReadFile(index); bytes.Equal(before, after) {
		t.Fatal("control: git status did not rewrite the stale index")
	}
	_ = os.Remove(gitRecord)
	before = stale()
	w.recordingGo()
	out, code, stderr := w.index(w.caller(nil), root)
	if code != 0 || baseEntry(t, out).Status != "ok" {
		t.Fatalf("exit %d, base %+v: %s", code, baseEntry(t, out), stderr)
	}
	if log, _ := os.ReadFile(gitRecord); strings.Contains(string(log), "status") {
		t.Errorf("the adapter's run ran git status: %q", log)
	}
	if after, _ := os.ReadFile(index); !bytes.Equal(before, after) {
		t.Errorf("the adapter's run rewrote .git/index")
	}
	for _, inv := range readRecord(t, w.gorec)[1:] {
		if !strings.Contains(inv.vars["GOFLAGS"], "-buildvcs=false") {
			t.Errorf("%v: GOFLAGS=%q", inv.args, inv.vars["GOFLAGS"])
		}
	}
}

func TestEnvironmentVendor(t *testing.T) {
	w := newWorld(t)
	w.recordingGo()
	root := w.flow("go-flow")
	w.goIn(root, nil, "mod", "vendor")
	empty := filepath.Join(w.dir, "empty-cache")
	out, code, stderr := w.index(w.caller(map[string]string{"GOMODCACHE": empty}), root)
	if code != 0 || baseEntry(t, out).Status != "ok" {
		t.Fatalf("exit %d, base %+v: %s", code, baseEntry(t, out), stderr)
	}
	record := readRecord(t, w.gorec)
	if len(record) < 2 {
		t.Fatalf("record %+v", record)
	}
	checkGo(t, record[1:], root, out, true)
	for _, inv := range readRecord(t, w.record) {
		checkChild(t, inv, root, out, true, stubAdded)
	}
	// Through the engine: a snapshot is published, with the external row of the x/mod function
	// the fixture calls. The stub serves the checked-in index, so the row's attribution is the
	// real indexer's; the live check runs that.
	snap := filepath.Join(w.dir, "snapshot")
	if code, stderr := w.engineWith(map[string]string{"GOMODCACHE": empty}, "index", "--root", root, "--out", snap); code != 0 {
		t.Fatalf("gosanja index exit %d: %s", code, stderr)
	}
	found := false
	for _, e := range jsonl(t, filepath.Join(snap, "externals.jsonl")) {
		found = found || e["symbol"] == "scip-go gomod golang.org/x/mod v0.43.0 `golang.org/x/mod/semver`/IsValid()."
	}
	if !found {
		t.Errorf("no external row for semver.IsValid")
	}
}

// A module cache spelled through a link and `..` is the directory the link resolves to; the go
// command, which joins paths lexically, would read the same spelling as a directory in the
// root. Downloading into a cold cache must fill the resolved directory and leave the root as
// it was.
func TestEnvironmentLinkDotDot(t *testing.T) {
	w := newWorld(t)
	root := w.flow("go-flow")
	outside := filepath.Join(w.dir, "outside", "deep")
	if err := os.MkdirAll(outside, 0o755); err != nil {
		t.Fatal(err)
	}
	link(t, outside, filepath.Join(root, "sub", "link"))
	// Read lexically, the spelling names root/sub/cache, whose cache entry links out of the
	// root, so the cache/download and cache/vcs beneath that reading pass the check too.
	elsewhere := filepath.Join(w.dir, "elsewhere")
	if err := os.Mkdir(elsewhere, 0o755); err != nil {
		t.Fatal(err)
	}
	link(t, elsewhere, filepath.Join(root, "sub", "cache", "cache"))
	spelled := filepath.Join(root, "sub", "link") + "/../cache"
	resolved := filepath.Join(w.dir, "outside", "cache")
	t.Cleanup(func() {
		_ = filepath.WalkDir(resolved, func(path string, d fs.DirEntry, err error) error {
			if err == nil && d.IsDir() {
				_ = os.Chmod(path, 0o755)
			}
			return nil
		})
	})
	before := listing(t, root)
	proxy := "file://" + filepath.Join(moduleCache(t), "cache", "download")
	out, code, stderr := w.index(w.caller(map[string]string{"GOMODCACHE": spelled, "GOPROXY": proxy}), root)
	if code != 0 || baseEntry(t, out).Status != "ok" {
		t.Fatalf("exit %d, base %+v: %s", code, baseEntry(t, out), stderr)
	}
	if after := listing(t, root); !reflect.DeepEqual(before, after) {
		t.Errorf("the root changed:\n%v\n%v", before, after)
	}
	if _, err := os.Stat(filepath.Join(resolved, "golang.org", "x", "mod@v0.43.0")); err != nil {
		t.Errorf("the dependency is not in the resolved cache: %v", err)
	}
}

// vendor/modules.txt turns vendor mode on only when it is a regular file.
func TestVendorMarkerNotAFile(t *testing.T) {
	w := newWorld(t)
	w.recordingGo()
	root := w.simple("m", nil)
	if err := os.MkdirAll(filepath.Join(root, "vendor", "modules.txt"), 0o755); err != nil {
		t.Fatal(err)
	}
	out, code, stderr := w.index(w.caller(nil), root)
	if code != 0 || baseEntry(t, out).Status != "ok" {
		t.Fatalf("exit %d, base %+v: %s", code, baseEntry(t, out), stderr)
	}
	record := readRecord(t, w.gorec)
	if len(record) < 2 {
		t.Fatalf("record %+v", record)
	}
	for _, inv := range record[1:] {
		if inv.vars["GOFLAGS"] != "-buildvcs=false -mod=readonly" {
			t.Errorf("%v: GOFLAGS=%q", inv.args, inv.vars["GOFLAGS"])
		}
	}
}
