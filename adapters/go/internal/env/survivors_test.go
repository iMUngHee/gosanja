package env

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// An explicit GOROOT that equals $HOME/go keeps the default GOPATH empty, as the go command's
// rule does; any other GOROOT leaves $HOME/go in place.
func TestDefaultGOPATHDoesNotUseGOROOT(t *testing.T) {
	home := t.TempDir()
	goRoot := filepath.Join(home, "go")
	caller := Caller{Vars: map[string]string{"HOME": home, "GOROOT": goRoot}}
	if got := caller.DefaultGOPATH(); got != "" {
		t.Fatalf("default GOPATH = %q, want an empty path", got)
	}
	caller = Caller{Vars: map[string]string{"HOME": home, "GOROOT": filepath.Join(home, "sdk")}}
	if got := caller.DefaultGOPATH(); got != goRoot {
		t.Fatalf("default GOPATH = %q, want %q", got, goRoot)
	}
}

// Create turns telemetry off in both configuration directories a child may use: Linux's
// XDG_CONFIG_HOME and Darwin's Library under the home.
func TestWorkDisablesTelemetryInBothConfigLocations(t *testing.T) {
	work := Work{Dir: t.TempDir()}
	if err := work.Create(); err != nil {
		t.Fatal(err)
	}
	for _, config := range []string{work.Config(), filepath.Join(work.Home(), "Library", "Application Support")} {
		path := filepath.Join(config, "go", "telemetry", "mode")
		if mode, err := os.ReadFile(path); err != nil || string(mode) != "off\n" {
			t.Errorf("mode in %s = %q, err = %v", config, mode, err)
		}
	}
}

// Go takes the first executable go on the caller's PATH, passing over a relative entry, a
// directory without go and a go that is not executable.
func TestGoChoosesExecutableOnCallerPath(t *testing.T) {
	missing, disabled, valid := t.TempDir(), t.TempDir(), t.TempDir()
	if err := os.WriteFile(filepath.Join(disabled, "go"), nil, 0o600); err != nil {
		t.Fatal(err)
	}
	want := filepath.Join(valid, "go")
	if err := os.WriteFile(want, nil, 0o700); err != nil {
		t.Fatal(err)
	}
	caller := Caller{Vars: map[string]string{
		"PATH": strings.Join([]string{"relative", missing, disabled, valid}, string(os.PathListSeparator)),
	}}
	if got := Go(caller); got != want {
		t.Fatalf("go executable = %q, want %q", got, want)
	}
}
