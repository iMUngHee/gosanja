// Package build derives targets and file membership from the base layer's Go inventory.
package build

import (
	"encoding/json"
	"fmt"
	"io"
	"maps"
	"path"
	"slices"
	"strings"
	"unicode"

	"gosanja/adapters/go/internal/base"
	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/scipindex"
)

type targetRow struct {
	Row  string      `json:"row"`
	Key  string      `json:"key"`
	Kind string      `json:"kind"`
	Name string      `json:"name"`
	Root *targetRoot `json:"root,omitempty"`
	Test bool        `json:"test,omitempty"`
}

type targetRoot struct {
	Symbol string `json:"symbol"`
}

type fileRow struct {
	Row        string   `json:"row"`
	File       string   `json:"file"`
	Targets    []string `json:"targets,omitempty"`
	Test       string   `json:"test,omitempty"`
	Constraint string   `json:"constraint,omitempty"`
}

type target struct {
	row     targetRow
	files   map[string]bool
	symbols map[string]bool
}

type namespace struct {
	path, symbol string
}

// Layer writes the build sidecar from the successful base layer's retained index, reads,
// and inventory. It does not load the source or run any tools.
func Layer(w io.Writer, root string, outcome base.Outcome) (contract.Entry, error) {
	clauses := packageClauses(outcome)
	inventory := outcome.Inventory.Packages
	byImport := make(map[string][]base.Package, len(inventory))
	for _, p := range inventory {
		if p.Main() && p.ForTest != "" {
			byImport[p.ImportPath] = append(byImport[p.ImportPath], p)
		}
	}

	targets := map[string]*target{}
	production, testFiles := map[string]bool{}, map[string]bool{}
	retained := func(p base.Package, names ...[]string) map[string]bool {
		files := map[string]bool{}
		for _, list := range names {
			for _, name := range list {
				if file, ok := p.RelativeFile(root, name); ok && outcome.Documents[file] {
					files[file] = true
				}
			}
		}
		return files
	}
	addTarget := func(p base.Package, kind string, files, rootFiles map[string]bool, rootImport string) {
		key := targetKey(p.Module.Path, p.ImportPath, kind)
		t := targets[key]
		if t == nil {
			t = &target{
				row:     targetRow{Row: "target", Key: key, Kind: kind, Name: p.ImportPath, Test: kind == "test"},
				files:   map[string]bool{},
				symbols: map[string]bool{},
			}
			targets[key] = t
		}
		maps.Copy(t.files, files)
		for file := range rootFiles {
			for _, clause := range clauses[file] {
				if clause.path == rootImport {
					t.symbols[clause.symbol] = true
				}
			}
		}
	}

	for _, p := range inventory {
		if !p.Project() || p.ForTest != "" || p.Name == "" || p.Module.Path == "" {
			continue
		}
		ordinary := retained(p, p.GoFiles)
		maps.Copy(production, ordinary)
		if len(p.GoFiles) != 0 {
			kind := "package"
			if p.Name == "main" {
				kind = "command"
			}
			addTarget(p, kind, ordinary, ordinary, p.ImportPath)
		}
		maps.Copy(testFiles, retained(p, p.TestGoFiles, p.XTestGoFiles))

		suffix := " [" + p.ImportPath + ".test]"
		internal, external := map[string]bool{}, map[string]bool{}
		internalVariant, externalVariant := false, false
		for _, variant := range byImport[p.ImportPath+suffix] {
			if variant.ForTest == p.ImportPath && variant.Dir == p.Dir && variant.Name == p.Name {
				internalVariant = true
				maps.Copy(internal, retained(variant, variant.GoFiles))
			}
		}
		for _, variant := range byImport[p.ImportPath+"_test"+suffix] {
			if variant.ForTest == p.ImportPath && variant.Dir == p.Dir && variant.Name == p.Name+"_test" {
				externalVariant = true
				maps.Copy(external, retained(variant, variant.GoFiles))
			}
		}
		if len(p.TestGoFiles)+len(p.XTestGoFiles) == 0 && !internalVariant && !externalVariant {
			continue
		}
		rootFiles, rootImport := ordinary, p.ImportPath
		if len(p.GoFiles) == 0 {
			rootFiles = internal
			if !internalVariant {
				rootFiles, rootImport = external, p.ImportPath+"_test"
			}
		}
		files := maps.Clone(ordinary)
		maps.Copy(files, internal)
		maps.Copy(files, external)
		for file := range files {
			if strings.HasSuffix(path.Base(file), "_test.go") {
				testFiles[file] = true
			}
		}
		addTarget(p, "test", files, rootFiles, rootImport)
	}

	keys := slices.Sorted(maps.Keys(targets))
	for _, key := range keys {
		if len(key) > 1024 {
			return contract.Failed("target-key-invalid"), nil
		}
	}
	encode := json.NewEncoder(w)
	membership := map[string][]string{}
	for _, key := range keys {
		t := targets[key]
		if len(t.symbols) == 1 {
			for symbol := range t.symbols {
				t.row.Root = &targetRoot{Symbol: symbol}
			}
		}
		if err := encode.Encode(t.row); err != nil {
			return contract.Entry{}, err
		}
		for file := range t.files {
			membership[file] = append(membership[file], key)
		}
	}

	var skipped []contract.Skipped
	for _, file := range slices.Sorted(maps.Keys(outcome.Documents)) {
		read := outcome.Reads[file]
		row := fileRow{Row: "file", File: file, Targets: membership[file], Constraint: read.Constraint}
		if testFiles[file] && !production[file] {
			row.Test = "file"
		}
		if read.MultipleConstraints {
			skipped = append(skipped, contract.Skipped{File: file, Code: "constraint-unreadable"})
		}
		if len(row.Targets) == 0 {
			skipped = append(skipped, contract.Skipped{File: file, Code: "target-unknown"})
			if row.Test == "" && row.Constraint == "" {
				continue
			}
		}
		if err := encode.Encode(row); err != nil {
			return contract.Entry{}, err
		}
	}
	return contract.OK(skipped), nil
}

func packageClauses(outcome base.Outcome) map[string][]namespace {
	clauses := map[string][]namespace{}
	for _, doc := range outcome.Index.Documents {
		read := outcome.Reads[doc.RelativePath]
		for _, occ := range doc.Occurrences {
			if !read.AtPackageClause(doc, occ) {
				continue
			}
			if path, ok := scipindex.NamespaceDefinition(occ); ok {
				clauses[doc.RelativePath] = append(clauses[doc.RelativePath], namespace{path, occ.Symbol})
			}
		}
	}
	return clauses
}

func targetKey(module, importPath, kind string) string {
	raw := "gomod:" + module + ":" + importPath + ":" + kind
	var key strings.Builder
	for _, r := range raw {
		if r == '%' || r == '\ufeff' || unicode.IsSpace(r) || unicode.IsControl(r) {
			for _, b := range []byte(string(r)) {
				fmt.Fprintf(&key, "%%%02X", b)
			}
		} else {
			key.WriteRune(r)
		}
	}
	return key.String()
}
