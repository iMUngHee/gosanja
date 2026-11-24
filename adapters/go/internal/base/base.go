// Package base makes the base layer: the preconditions in their order, the inventory of the
// build context, the scip-go run, and the checks that an exit status of 0 is not trusted for.
package base

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/env"
	"gosanja/adapters/go/internal/paths"
	"gosanja/adapters/go/internal/scipindex"
)

// Config is what a run of the base layer is given.
type Config struct {
	Root   string // resolved
	Out    string // resolved
	GOOS   string
	Caller env.Caller
	Stderr io.Writer
}

// Outcome is the base layer's result and what the later layers read.
type Outcome struct {
	Entry     contract.Entry
	Tools     []contract.Tool
	Env       []string
	Inventory *Inventory
	Reads     map[string]Read // module files of project packages, by relative path
	Documents map[string]bool
	Index     *scipindex.Index // the decoded index, when the base layer read one
}

// Failed reports whether the base layer failed.
func (o Outcome) Failed() bool { return o.Entry.Status != "ok" }

type failure struct {
	code, detail string
}

func (f *failure) Error() string { return f.code + ": " + f.detail }

func fail(code, format string, args ...any) *failure {
	return &failure{code: code, detail: fmt.Sprintf(format, args...)}
}

// Run makes the base layer. A precondition that fails ends the run with its code; the tool's
// output goes to standard error after a line naming the code. An error is returned only when
// the adapter cannot do its own work, such as writing under its output directory.
func Run(ctx context.Context, cfg Config) (Outcome, error) {
	var out Outcome
	err := run(ctx, cfg, &out)
	var f *failure
	if errors.As(err, &f) {
		fmt.Fprintln(cfg.Stderr, f.code)
		if f.detail != "" {
			fmt.Fprintln(cfg.Stderr, f.detail)
		}
		out.Entry = contract.Failed(f.code)
		return out, nil
	}
	return out, err
}

var requiresGo = regexp.MustCompile(`go\.mod requires go >= \S+`)

func run(ctx context.Context, cfg Config, out *Outcome) error {
	if cfg.GOOS == "windows" {
		return fail("platform-unsupported", "the Go adapter does not run on Windows")
	}
	if info, err := os.Stat(filepath.Join(cfg.Root, "go.mod")); err != nil || !info.Mode().IsRegular() {
		return fail("manifest-missing", "no go.mod in %s", cfg.Root)
	}
	work := env.Work{Dir: filepath.Join(cfg.Out, "work")}
	if err := work.Create(); err != nil {
		return err
	}
	values, err := env.Bootstrap(ctx, cfg.Caller, work)
	if err != nil {
		return fail("toolchain-missing", "%v", err)
	}
	if err := checkDestinations(cfg, values); err != nil {
		return err
	}
	_, vendorErr := os.Stat(filepath.Join(cfg.Root, "vendor", "modules.txt"))
	out.Env = env.Child(cfg.Caller, values, work, vendorErr == nil)
	goTool := env.Go(cfg.Caller)
	scipGo := lookScipGo(cfg.Caller)

	scipVersion, err := probe(ctx, cfg, out.Env, scipGo, "--version")
	if err != nil {
		return fail("toolchain-missing", "scip-go --version: %v", err)
	}
	goVersion, err := probe(ctx, cfg, out.Env, goTool, "version")
	if err != nil {
		return fail("toolchain-missing", "go version: %v", err)
	}
	out.Tools = []contract.Tool{{Name: "go", Version: goWord(goVersion)}, {Name: "scip-go", Version: scipVersion}}

	if stdout, stderr, err := runIn(ctx, cfg.Root, out.Env, goTool, "list", "-m", "-json"); err != nil || !oneObject(stdout) {
		switch {
		case requiresGo.Match(stderr):
			return fail("toolchain-too-old", "%s", strings.TrimSpace(string(stderr)))
		case bytes.HasPrefix(stderr, []byte("go: inconsistent vendoring")):
			return fail("vendor-inconsistent", "%s", strings.TrimSpace(string(stderr)))
		default:
			return fail("manifest-invalid", "go list -m -json: %v: %s", err, strings.TrimSpace(string(stderr)))
		}
	}
	stdout, stderr, err := runIn(ctx, cfg.Root, out.Env, goTool, "mod", "edit", "-json")
	var modfile struct {
		Replace []struct{ New struct{ Path string } }
		Ignore  []struct{ Path string }
	}
	if err != nil || !oneObject(stdout) || json.Unmarshal(stdout, &modfile) != nil {
		return fail("manifest-invalid", "go mod edit -json: %v: %s", err, strings.TrimSpace(string(stderr)))
	}
	for _, r := range modfile.Replace {
		if filepath.IsAbs(r.New.Path) {
			return fail("replace-absolute", "go.mod replaces a module with the absolute directory %s", r.New.Path)
		}
	}
	stdout, stderr, err = runIn(ctx, cfg.Root, out.Env, goTool, "list", "-e", "-test", "-deps",
		"-json="+inventoryFields, "./...")
	if err != nil {
		return fail("dependencies-unreadable", "go list: %v: %s", err, strings.TrimSpace(string(stderr)))
	}
	inv, err := decodeInventory(stdout)
	if err != nil {
		return fail("dependencies-unreadable", "go list output: %v", err)
	}
	if detail, ok := inv.dependencyError(); ok {
		return fail("dependencies-missing", "%s", detail)
	}
	out.Inventory = inv
	return index(ctx, cfg, out, inv, modfile.Ignore, scipGo, work)
}

func index(ctx context.Context, cfg Config, out *Outcome, inv *Inventory, ignore []struct{ Path string }, scipGo string, work env.Work) error {
	rootInfo, err := os.Stat(cfg.Root)
	if err != nil {
		return err
	}
	out.Reads = map[string]Read{}
	var skipped []contract.Skipped
	for _, f := range inv.moduleFiles(cfg.Root, sources) {
		read := readFile(rootInfo, f)
		out.Reads[f.rel] = read
		if read.Code != "" {
			skipped = append(skipped, contract.Skipped{File: f.rel, Code: read.Code})
		}
	}
	for _, f := range inv.moduleFiles(cfg.Root, excluded) {
		skipped = append(skipped, contract.Skipped{File: f.rel, Code: "build-constraint"})
	}
	var ignored []string
	for _, i := range ignore {
		ignored = append(ignored, i.Path)
	}
	constrained, missed, err := unnamed(cfg.Root, inv, ignored)
	if err != nil {
		return err
	}
	if len(missed) > 0 {
		return fail("index-incomplete", "the package list names no package for %s, which the build context builds", strings.Join(missed, ", "))
	}
	for _, rel := range constrained {
		skipped = append(skipped, contract.Skipped{File: rel, Code: "build-constraint"})
	}

	output := filepath.Join(work.Dir, "index.scip")
	if _, stderr, err := runIn(ctx, cfg.Root, out.Env, scipGo, "index", "--module-version", "v0.0.0", "--output", output); err != nil {
		return fail("indexer-failed", "scip-go index: %v: %s", err, strings.TrimSpace(string(stderr)))
	}
	if _, err := os.Stat(output); err != nil {
		return fail("indexer-failed", "scip-go wrote no index")
	}
	idx, err := scipindex.Read(output)
	if err != nil {
		return fail("index-unreadable", "%v", err)
	}
	out.Index = idx
	out.Documents = map[string]bool{}
	for _, p := range idx.Paths() {
		out.Documents[p] = true
	}
	if len(out.Documents) == 0 {
		return fail("index-incomplete", "the index has no document")
	}
	var absent []string
	for rel, read := range out.Reads {
		if read.Code == "" && !out.Documents[rel] {
			absent = append(absent, rel)
		}
	}
	if len(absent) > 0 {
		sort.Strings(absent)
		return fail("index-incomplete", "the index has no document for %s", strings.Join(absent, ", "))
	}
	if err := os.Rename(output, filepath.Join(cfg.Out, "index.scip")); err != nil {
		return err
	}
	out.Entry = contract.OK(sortSkipped(skipped))
	return nil
}

func sortSkipped(s []contract.Skipped) []contract.Skipped {
	sort.Slice(s, func(i, j int) bool {
		if s[i].File != s[j].File {
			return s[i].File < s[j].File
		}
		return s[i].Code < s[j].Code
	})
	var out []contract.Skipped
	for i, x := range s {
		if i == 0 || x != s[i-1] {
			out = append(out, x)
		}
	}
	return out
}

// checkDestinations refuses a write destination of the go tool that is the root or lies
// beneath it, judged by file identity.
func checkDestinations(cfg Config, v env.Values) error {
	root, err := os.Stat(cfg.Root)
	if err != nil {
		return err
	}
	var candidates []string
	for _, entry := range filepath.SplitList(v["GOPATH"]) {
		if entry != "" {
			candidates = append(candidates, entry)
		}
	}
	if first := env.FirstGOPATH(v["GOPATH"]); first != "" {
		candidates = append(candidates, filepath.Join(first, "pkg", "sumdb"))
	}
	if cache := v["GOMODCACHE"]; cache != "" {
		candidates = append(candidates, cache, filepath.Join(cache, "cache", "download"), filepath.Join(cache, "cache", "vcs"))
	}
	for _, c := range candidates {
		resolved, err := paths.Resolve(c, cfg.Caller.Cwd)
		if err != nil {
			return fail("tool-home-unresolvable", "%v", err)
		}
		inside, err := paths.Within(root, resolved)
		if err != nil {
			return fail("tool-home-unresolvable", "%v", err)
		}
		if inside {
			return fail("tool-home-inside-root", "%s lies in the root %s", c, cfg.Root)
		}
	}
	return nil
}

func lookScipGo(c env.Caller) string {
	for _, dir := range filepath.SplitList(c.Vars["PATH"]) {
		if dir == "" || !filepath.IsAbs(dir) {
			continue
		}
		candidate := filepath.Join(dir, "scip-go")
		if info, err := os.Stat(candidate); err == nil && !info.IsDir() && info.Mode()&0o111 != 0 {
			return candidate
		}
	}
	return "scip-go"
}

// probe runs a version query and returns its first line.
func probe(ctx context.Context, cfg Config, vars []string, tool string, args ...string) (string, error) {
	stdout, stderr, err := runIn(ctx, cfg.Root, vars, tool, args...)
	if err != nil {
		return "", fmt.Errorf("%v: %s", err, strings.TrimSpace(string(stderr)))
	}
	line, _, _ := strings.Cut(strings.TrimSpace(string(stdout)), "\n")
	if line == "" {
		return "", errors.New("it printed no version")
	}
	return line, nil
}

// goWord takes the version word of `go version go1.27.3 linux/amd64`.
func goWord(line string) string {
	fields := strings.Fields(line)
	if len(fields) >= 3 && fields[0] == "go" && fields[1] == "version" {
		return fields[2]
	}
	return line
}

func runIn(ctx context.Context, dir string, vars []string, tool string, args ...string) (stdout, stderr []byte, err error) {
	cmd := exec.CommandContext(ctx, tool, args...)
	cmd.Dir, cmd.Env = dir, vars
	var o, e bytes.Buffer
	cmd.Stdout, cmd.Stderr = &o, &e
	err = cmd.Run()
	return o.Bytes(), e.Bytes(), err
}

func oneObject(b []byte) bool {
	dec := json.NewDecoder(bytes.NewReader(b))
	var v map[string]any
	return dec.Decode(&v) == nil && !dec.More()
}
