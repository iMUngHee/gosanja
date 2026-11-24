package base

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"path/filepath"
	"sort"
	"strings"
)

// inventoryFields are the fields the inventory selects. Selecting fields keeps go list from
// stamping VCS information, which runs git in the root.
const inventoryFields = "ImportPath,Dir,Module,DepOnly,GoFiles,TestGoFiles,XTestGoFiles,IgnoredGoFiles,IgnoredOtherFiles,CgoFiles,InvalidGoFiles,Error"

// Package is one package of the inventory, as `go list -json` prints it.
type Package struct {
	ImportPath        string
	Dir               string
	Module            *struct{ Main bool }
	DepOnly           bool
	GoFiles           []string
	TestGoFiles       []string
	XTestGoFiles      []string
	IgnoredGoFiles    []string
	IgnoredOtherFiles []string
	CgoFiles          []string
	InvalidGoFiles    []string
	Error             *struct{ Pos, Err string }
}

// Main reports whether the package belongs to the main module.
func (p Package) Main() bool { return p.Module != nil && p.Module.Main }

// Project reports whether `./...` names the package as a source package.
func (p Package) Project() bool {
	return p.Main() && !p.DepOnly && !p.testRunner()
}

// go list names the synthetic runner's source _testmain.go or an absolute
// build-cache path. A real package ending in .test has relative source names.
func (p Package) testRunner() bool {
	return strings.HasSuffix(p.ImportPath, ".test") && len(p.GoFiles) == 1 &&
		(p.GoFiles[0] == "_testmain.go" || filepath.IsAbs(p.GoFiles[0]))
}

// Inventory is the decoded output of the inventory command.
type Inventory struct {
	Packages []Package
}

func decodeInventory(out []byte) (*Inventory, error) {
	dec := json.NewDecoder(bytes.NewReader(out))
	var inv Inventory
	for {
		var p Package
		err := dec.Decode(&p)
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return nil, err
		}
		inv.Packages = append(inv.Packages, p)
	}
	return &inv, nil
}

// dependencyError is the first error that originates outside the main module. Each package
// that -deps lists carries its own errors; DepsErrors are copies held by importers, and are
// not read.
func (inv *Inventory) dependencyError() (string, bool) {
	for _, p := range inv.Packages {
		if p.Error != nil && !p.Main() {
			return p.ImportPath + ": " + p.Error.Err, true
		}
	}
	return "", false
}

// file is a module file: its slash path relative to the root and its absolute path.
type file struct {
	rel, abs string
}

// moduleFiles lists the module files of project packages named by the given lists, each once,
// sorted by relative path. Names are resolved against each package's Dir.
func (inv *Inventory) moduleFiles(root string, lists func(Package) [][]string) []file {
	seen := map[string]bool{}
	var out []file
	for _, p := range inv.Packages {
		if !p.Project() {
			continue
		}
		for _, list := range lists(p) {
			for _, name := range list {
				abs := filepath.Join(p.Dir, name)
				rel, err := filepath.Rel(root, abs)
				if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
					continue
				}
				rel = filepath.ToSlash(rel)
				if !seen[rel] {
					seen[rel] = true
					out = append(out, file{rel: rel, abs: abs})
				}
			}
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].rel < out[j].rel })
	return out
}

// sources are the files the adapter reads itself and requires in the index when readable.
func sources(p Package) [][]string {
	return [][]string{p.GoFiles, p.TestGoFiles, p.XTestGoFiles, p.InvalidGoFiles}
}

// excluded are the Go files the build context leaves out.
func excluded(p Package) [][]string { return [][]string{p.IgnoredGoFiles, p.CgoFiles} }

// named reports every file name any package of the inventory names, as absolute paths.
func (inv *Inventory) named() map[string]bool {
	names := map[string]bool{}
	for _, p := range inv.Packages {
		if p.testRunner() {
			continue
		}
		for _, list := range [][]string{p.GoFiles, p.TestGoFiles, p.XTestGoFiles, p.IgnoredGoFiles,
			p.IgnoredOtherFiles, p.CgoFiles, p.InvalidGoFiles} {
			for _, name := range list {
				names[filepath.Join(p.Dir, name)] = true
			}
		}
	}
	return names
}

// Owned maps each project package ID to its sources and invalid files.
// GoFiles contains tests only in their compiling variant. InvalidGoFiles
// keeps diagnostics for files Go could not read or parse with that package.
func (inv *Inventory) Owned(root string) map[string][]string {
	owned := map[string][]string{}
	for _, p := range inv.Packages {
		if !p.Project() {
			continue
		}
		for _, members := range [][]string{p.GoFiles, p.InvalidGoFiles} {
			for _, name := range members {
				rel, err := filepath.Rel(root, filepath.Join(p.Dir, name))
				if err == nil && !strings.HasPrefix(rel, "..") {
					owned[p.ImportPath] = append(owned[p.ImportPath], filepath.ToSlash(rel))
				}
			}
		}
		sort.Strings(owned[p.ImportPath])
	}
	return owned
}
