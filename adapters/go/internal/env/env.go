// Package env builds the environment of every Go process the adapter starts: the caller's Go
// settings, read once, and the destinations the adapter owns under its output directory.
package env

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// Settings names the eight Go settings the adapter takes from the caller, in the order the
// bootstrap query lists them.
var Settings = []string{"GOMODCACHE", "GOPATH", "GOPROXY", "GOPRIVATE", "GONOPROXY", "GONOSUMDB", "GOSUMDB", "GOINSECURE"}

// passed are the caller's variables every child receives unchanged when they are set.
var passed = []string{
	"PATH",
	"HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY",
	"http_proxy", "https_proxy", "all_proxy", "no_proxy",
	"SSL_CERT_FILE", "SSL_CERT_DIR",
}

// Caller is the environment the adapter was started in.
type Caller struct {
	Vars map[string]string
	Cwd  string
	// ConfigDir is os.UserConfigDir under the caller's environment.
	ConfigDir func() (string, error)
}

// FromProcess reads the adapter's own environment.
func FromProcess() (Caller, error) {
	cwd, err := os.Getwd()
	if err != nil {
		return Caller{}, err
	}
	vars := map[string]string{}
	for _, kv := range os.Environ() {
		if k, v, ok := strings.Cut(kv, "="); ok {
			vars[k] = v
		}
	}
	return Caller{Vars: vars, Cwd: cwd, ConfigDir: os.UserConfigDir}, nil
}

// EnvFile is the caller's Go env file, computed without running Go: `off` stays `off`, an
// absolute GOENV is kept, a relative one is taken against the caller's working directory,
// since Go resolves it against its own; otherwise the file `go env -w` writes.
func (c Caller) EnvFile() (string, error) {
	switch goenv := c.Vars["GOENV"]; {
	case goenv == "off":
		return "off", nil
	case goenv != "" && filepath.IsAbs(goenv):
		return goenv, nil
	case goenv != "":
		return filepath.Join(c.Cwd, goenv), nil
	}
	dir, err := c.ConfigDir()
	if err != nil {
		return "", fmt.Errorf("the Go env file has no location: %w", err)
	}
	return filepath.Join(dir, "go", "env"), nil
}

// DefaultGOPATH is Go's default GOPATH for the caller, `$HOME/go`, or nothing when HOME is unset
// or that directory is the Go root (go/build's defaultGOPATH).
func (c Caller) DefaultGOPATH() string {
	home := c.Vars["HOME"]
	if home == "" {
		return ""
	}
	def := filepath.Join(home, "go")
	if goroot := c.Vars["GOROOT"]; goroot != "" && filepath.Clean(goroot) == def {
		return ""
	}
	return def
}

// Work holds the directories the adapter owns under its output directory.
type Work struct{ Dir string }

func (w Work) Home() string   { return filepath.Join(w.Dir, "home") }
func (w Work) Config() string { return filepath.Join(w.Dir, "config") }
func (w Work) Cache() string  { return filepath.Join(w.Dir, "cache") }
func (w Work) Tmp() string    { return filepath.Join(w.Dir, "tmp") }

// Create makes the directories.
func (w Work) Create() error {
	for _, dir := range []string{w.Home(), w.Config(), w.Cache(), w.Tmp()} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	return nil
}

func (w Work) owned() []string {
	return []string{
		"HOME=" + w.Home(), "XDG_CONFIG_HOME=" + w.Config(), "GOCACHE=" + w.Cache(),
		"TMPDIR=" + w.Tmp(), "GOTMPDIR=" + w.Tmp(),
		"GOTOOLCHAIN=local", "GOWORK=off", "GOPACKAGESDRIVER=off",
	}
}

// Values are the caller's effective Go settings, as the bootstrap returned them.
type Values map[string]string

// ErrBootstrap reports that the first go command failed or answered something else than the
// eight settings.
var ErrBootstrap = errors.New("the first go command failed")

// Bootstrap runs `go env -json` for the eight settings once, in the work directory, with the
// caller's env file and exported settings, every write redirected under the work directory.
// A GOPATH or GOMODCACHE that comes back defaulted under the redirected home was set nowhere,
// and is replaced by the caller's default.
func Bootstrap(ctx context.Context, c Caller, w Work) (Values, error) {
	file, err := c.EnvFile()
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrBootstrap, err)
	}
	var vars []string
	for _, k := range passed {
		if v, ok := c.Vars[k]; ok {
			vars = append(vars, k+"="+v)
		}
	}
	for _, k := range Settings {
		if v, ok := c.Vars[k]; ok {
			vars = append(vars, k+"="+v)
		}
	}
	vars = append(vars, "GOENV="+file)
	vars = append(vars, w.owned()...)
	cmd := exec.CommandContext(ctx, lookGo(c), append([]string{"env", "-json"}, Settings...)...)
	cmd.Dir, cmd.Env = w.Dir, vars
	var stdout, stderr bytes.Buffer
	cmd.Stdout, cmd.Stderr = &stdout, &stderr
	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("%w: %v: %s", ErrBootstrap, err, strings.TrimSpace(stderr.String()))
	}
	var got map[string]string
	dec := json.NewDecoder(&stdout)
	if err := dec.Decode(&got); err != nil || dec.More() {
		return nil, fmt.Errorf("%w: its output is not one JSON object", ErrBootstrap)
	}
	if len(got) != len(Settings) {
		return nil, fmt.Errorf("%w: it answered %d settings", ErrBootstrap, len(got))
	}
	for _, k := range Settings {
		if _, ok := got[k]; !ok {
			return nil, fmt.Errorf("%w: it did not answer %s", ErrBootstrap, k)
		}
	}
	redirected := filepath.Join(w.Home(), "go")
	if got["GOPATH"] == redirected {
		got["GOPATH"] = c.DefaultGOPATH()
	}
	if got["GOMODCACHE"] == filepath.Join(redirected, "pkg", "mod") {
		got["GOMODCACHE"] = ""
		if first := FirstGOPATH(got["GOPATH"]); first != "" {
			got["GOMODCACHE"] = filepath.Join(first, "pkg", "mod")
		}
	}
	return Values(got), nil
}

// FirstGOPATH is the entry of a GOPATH list the go tool writes under.
func FirstGOPATH(gopath string) string {
	for _, entry := range filepath.SplitList(gopath) {
		return entry
	}
	return ""
}

// Child is the exact environment of every later child: the caller's PATH and network
// variables, the eight settings, GOENV=off, the owned directories and fixed modes.
func Child(c Caller, v Values, w Work, vendor bool) []string {
	var vars []string
	for _, k := range passed {
		if value, ok := c.Vars[k]; ok {
			vars = append(vars, k+"="+value)
		}
	}
	for _, k := range Settings {
		if v[k] != "" {
			vars = append(vars, k+"="+v[k])
		}
	}
	mod := "-mod=readonly"
	if vendor {
		mod = "-mod=vendor"
	}
	vars = append(vars, "GOENV=off", "CGO_ENABLED=0", "GOFLAGS=-buildvcs=false "+mod)
	vars = append(vars, w.owned()...)
	sort.Strings(vars)
	return vars
}

// Go is the go command the adapter runs, found on the caller's PATH.
func Go(c Caller) string { return lookGo(c) }

func lookGo(c Caller) string {
	for _, dir := range filepath.SplitList(c.Vars["PATH"]) {
		if dir == "" || !filepath.IsAbs(dir) {
			continue
		}
		candidate := filepath.Join(dir, "go")
		if info, err := os.Stat(candidate); err == nil && !info.IsDir() && info.Mode()&0o111 != 0 {
			return candidate
		}
	}
	return "go"
}
