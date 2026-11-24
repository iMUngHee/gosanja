//go:build live

// The live tests run the real scip-go and go of the caller's PATH, and inotifywait; the gate
// never builds them. scripts/check-live.sh runs them after checking the tools' versions.

package main

import (
	"bufio"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"
)

// realScipGo replaces the world's stub with the scip-go of the test's PATH.
func realScipGo(t *testing.T, w *world) {
	t.Helper()
	path, err := exec.LookPath("scip-go")
	if err != nil {
		t.Fatal("the live tests need scip-go on PATH")
	}
	if err := os.Remove(filepath.Join(w.bin, "scip-go")); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(path, filepath.Join(w.bin, "scip-go")); err != nil {
		t.Fatal(err)
	}
}

// watch starts inotifywait over dirs and returns a function that stops it and gives every
// event that changes something: a write, a creation, a removal, a move or a change of
// attributes.
func watch(t *testing.T, dirs ...string) func() []string {
	t.Helper()
	args := append([]string{"-m", "-r", "-e", "modify,close_write,create,delete,moved_to,moved_from,attrib",
		"--format", "%w%f %e"}, dirs...)
	cmd := exec.Command("inotifywait", args...)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		t.Fatal(err)
	}
	// inotifywait prints "Watches established." on standard error once the watches are in place.
	if err := cmd.Start(); err != nil {
		t.Fatalf("inotifywait: %v", err)
	}
	ready := make(chan struct{})
	go func() {
		scanner := bufio.NewScanner(stderr)
		for scanner.Scan() {
			if strings.Contains(scanner.Text(), "Watches established") {
				close(ready)
				break
			}
		}
		_, _ = io.Copy(io.Discard, stderr)
	}()
	select {
	case <-ready:
	case <-time.After(30 * time.Second):
		_ = cmd.Process.Kill()
		t.Fatal("inotifywait established no watch")
	}
	lines := make(chan []string, 1)
	go func() {
		var out []string
		scanner := bufio.NewScanner(stdout)
		for scanner.Scan() {
			out = append(out, scanner.Text())
		}
		lines <- out
	}()
	return func() []string {
		time.Sleep(500 * time.Millisecond) // let the last events arrive
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
		return <-lines
	}
}

// configDir is the caller's Go configuration directory, os.UserConfigDir()/go for the world's
// home, created so that it can be watched.
func configDir(t *testing.T, w *world) string {
	t.Helper()
	dir := filepath.Join(w.home, ".config", "go")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	return dir
}

func TestLiveGolden(t *testing.T) {
	w := newWorld(t)
	realScipGo(t, w)
	root := w.flow("go-flow")
	if err := os.Remove(filepath.Join(root, "index.scip")); err != nil {
		t.Fatal(err)
	}
	if err := os.RemoveAll(filepath.Join(root, "expected")); err != nil {
		t.Fatal(err)
	}
	config := configDir(t, w)

	// Control: a bare go command in the copy writes under the config directory.
	stop := watch(t, root, config)
	control := exec.Command(realGo(t), "env", "GOPROXY")
	control.Dir = root
	control.Env = []string{"PATH=" + w.bin + ":/usr/bin:/bin", "HOME=" + w.home}
	if out, err := control.CombinedOutput(); err != nil {
		t.Fatalf("control go env: %v\n%s", err, out)
	}
	if events := stop(); !hasPrefix(events, config) {
		t.Fatalf("control: go env GOPROXY wrote nothing under %s: %v", config, events)
	}

	readOnly(t, root)
	before := listing(t, root)
	stop = watch(t, root, config)
	out := filepath.Join(w.dir, "snapshot")
	code, output := w.engine("index", "--root", root, "--out", out)
	events := stop()
	if code != 0 {
		t.Fatalf("gosanja index exit %d: %s", code, output)
	}
	if len(events) != 0 {
		t.Errorf("events under the copy or the config directory during the run: %v", events)
	}
	if after := listing(t, root); !reflect.DeepEqual(before, after) {
		t.Errorf("the copy changed")
	}
	if differ := sameTree(t, out, filepath.Join(repo(t), "fixtures", "go-flow", "expected")); len(differ) != 0 {
		t.Errorf("the snapshot differs from fixtures/go-flow/expected in %v", differ)
	}
}

func hasPrefix(events []string, dir string) bool {
	for _, e := range events {
		if strings.HasPrefix(e, dir+"/") {
			return true
		}
	}
	return false
}

func TestLiveMissingModule(t *testing.T) {
	w := newWorld(t)
	realScipGo(t, w)
	proxy := newListener(t, "")
	root := w.module("m", map[string]string{
		"go.mod": "module example.com/m\n\ngo 1.26\n\nrequire example.invalid/dep v1.0.0\n",
		"go.sum": "example.invalid/dep v1.0.0 h1:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=\n" +
			"example.invalid/dep v1.0.0/go.mod h1:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=\n",
		"a.go": "package m\n\nimport _ \"example.invalid/dep\"\n",
	})
	empty := filepath.Join(w.dir, "empty-cache")
	extra := map[string]string{"GOPROXY": proxy.URL, "GOMODCACHE": empty, "GONOSUMDB": "example.invalid"}
	code, output := w.engineWith(extra, "index", "--root", root, "--out", filepath.Join(w.dir, "snapshot"))
	if code != 3 || !strings.Contains(output, "base layer failed: dependencies-missing") {
		t.Fatalf("gosanja index exit %d: %s", code, output)
	}
	if proxy.count() == 0 {
		t.Errorf("the listener received no request")
	}
}

func TestLiveToolchainLine(t *testing.T) {
	w := newWorld(t)
	realScipGo(t, w)
	proxy := newListener(t, "")
	root := w.flow("go-flow")
	gomod := filepath.Join(root, "go.mod")
	text, err := os.ReadFile(gomod)
	if err != nil {
		t.Fatal(err)
	}
	w.write(gomod, strings.Replace(string(text), "go 1.27.3\n", "go 1.27.3\n\ntoolchain go1.99.1\n", 1), 0o644)

	// Control: under GOTOOLCHAIN=auto, go env in the copy asks the proxy for that toolchain.
	control := exec.Command(realGo(t), "env", "-json")
	control.Dir = root
	control.Env = []string{"PATH=/usr/bin:/bin", "HOME=" + w.home, "GOTOOLCHAIN=auto", "GOPROXY=" + proxy.URL,
		"GOMODCACHE=" + filepath.Join(w.dir, "control-cache"), "GOSUMDB=off"}
	_, _ = control.CombinedOutput()
	if proxy.count() == 0 {
		t.Fatal("control: go env -json under GOTOOLCHAIN=auto did not reach the listener")
	}
	seen := proxy.count()

	out := filepath.Join(w.dir, "snapshot")
	code, output := w.engineWith(map[string]string{"GOPROXY": proxy.URL}, "index", "--root", root, "--out", out)
	if code != 0 {
		t.Fatalf("gosanja index exit %d: %s", code, output)
	}
	if s := readSnapshot(t, out); s.layers["base"] != "present" {
		t.Errorf("layers %v", s.layers)
	}
	if proxy.count() != seen {
		t.Errorf("the run sent %d requests to the listener", proxy.count()-seen)
	}
}
