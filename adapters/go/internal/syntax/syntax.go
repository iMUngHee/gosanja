// Package syntax extracts written declarations from the Go files in the base index.
package syntax

import (
	"cmp"
	"context"
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"io"
	"path"
	"path/filepath"
	"slices"
	"strings"
	"unicode/utf8"

	"golang.org/x/tools/go/packages"
	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/scipindex"
)

// File is the base layer's own read of a module file. An empty Code means that a
// regular file inside the root parsed successfully.
type File struct {
	Code             string
	HasLineDirective bool
	MergedPackage    bool
}

// Input carries the canonical module root, child environment and base layer
// observations. File names are relative slash-separated paths. Inventory keys
// are Go package IDs, including test variants.
type Input struct {
	Root      string
	Env       []string
	Files     map[string]File
	Documents map[string]bool
	Inventory map[string][]string
	Stderr    io.Writer
	// Index supplies SCIP definition extents when outlines are requested.
	Index *scipindex.Index
}

// Layer loads the module and its test packages and extracts its syntax facts.
func Layer(ctx context.Context, in Input) (contract.Entry, []contract.SyntaxFact) {
	return loadLayer(ctx, in, packages.Load)
}

type loader func(*packages.Config, ...string) ([]*packages.Package, error)

func loadLayer(ctx context.Context, in Input, load loader) (contract.Entry, []contract.SyntaxFact) {
	cfg := &packages.Config{
		Context: ctx, Dir: in.Root, Env: append([]string{}, in.Env...), Tests: true,
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedSyntax |
			packages.NeedTypes | packages.NeedTypesInfo | packages.NeedModule,
	}
	loaded, err := load(cfg, "./...")
	if err != nil || len(loaded) == 0 {
		if err != nil && in.Stderr != nil {
			fmt.Fprintf(in.Stderr, "load Go syntax: %v\n", err)
		}
		return contract.Failed("load-failed"), nil
	}

	s := selection{in: in, skipped: map[contract.Skipped]bool{}, suppressed: map[string]bool{}, extents: definitionExtents(in.Index)}
	for file, read := range in.Files {
		if read.Code != "" && portable(file) {
			s.skip(file, read.Code)
		}
	}
	// scip-go prefers the variant with more syntax files in the package.
	// ID ordering breaks ties independently of loader arrival order.
	loaded = slices.Clone(loaded)
	slices.SortStableFunc(loaded, func(a, b *packages.Package) int {
		return cmp.Or(cmp.Compare(a.PkgPath, b.PkgPath), cmp.Compare(len(b.Syntax), len(a.Syntax)), cmp.Compare(a.ID, b.ID))
	})
	type chosenFile struct {
		tree *ast.File
		fset *token.FileSet
		info *types.Info
	}
	chosen := map[string]chosenFile{}
	for _, pkg := range loaded {
		files := s.members(pkg)
		parsed := map[string]bool{}
		for _, syntax := range pkg.Syntax {
			file := relativeSource(in.Root, pkg.Fset.PositionFor(syntax.Package, false).Filename)
			if _, ok := in.Files[file]; !ok || file == "" {
				continue
			}
			files[file], parsed[file] = true, true
			if _, exists := chosen[file]; !exists && in.Documents[file] && in.Files[file].Code == "" {
				chosen[file] = chosenFile{syntax, pkg.Fset, pkg.TypesInfo}
			}
		}
		s.account(pkg, files, parsed)
	}

	var rows []contract.SyntaxFact
	for file, choice := range chosen {
		if !s.suppressed[file] {
			rows = append(rows, s.items(file, choice.tree, choice.fset, choice.info)...)
		}
	}
	slices.SortFunc(rows, func(a, b contract.SyntaxFact) int {
		x, y := a.Anchor, b.Anchor
		return cmp.Or(cmp.Compare(x.File, y.File), cmp.Compare(x.Line, y.Line), cmp.Compare(x.Col, y.Col))
	})
	skips := make([]contract.Skipped, 0, len(s.skipped))
	for skip := range s.skipped {
		skips = append(skips, skip)
	}
	slices.SortFunc(skips, func(a, b contract.Skipped) int {
		return cmp.Or(cmp.Compare(a.File, b.File), cmp.Compare(a.Code, b.Code))
	})
	return contract.OK(skips), rows
}

type selection struct {
	in         Input
	skipped    map[contract.Skipped]bool
	suppressed map[string]bool
	extents    map[contract.Anchor]contract.Range
}

func (s *selection) skip(file, code string) {
	s.skipped[contract.Skipped{File: file, Code: code}] = true
}

func (s *selection) members(pkg *packages.Package) map[string]bool {
	files := map[string]bool{}
	for _, file := range s.in.Inventory[pkg.ID] {
		if _, ok := s.in.Files[file]; ok && portable(file) {
			files[file] = true
		}
	}
	for _, filename := range pkg.GoFiles {
		file := relativeSource(s.in.Root, filename)
		if _, ok := s.in.Files[file]; ok && file != "" {
			files[file] = true
		}
	}
	return files
}

func relativeSource(root, filename string) string {
	if filename != "" {
		file, err := filepath.Rel(root, absolute(root, filename))
		file = filepath.ToSlash(file)
		if err == nil && portable(file) {
			return file
		}
	}
	return ""
}

func portable(file string) bool {
	if !utf8.ValidString(file) || strings.ContainsAny(file, "\\\x00") || strings.HasPrefix(file, "/") ||
		file == "." || file == ".." || strings.HasPrefix(file, "../") || path.Clean(file) != file {
		return false
	}
	return !(len(file) >= 3 && file[1:3] == ":/" &&
		('A' <= file[0] && file[0] <= 'Z' || 'a' <= file[0] && file[0] <= 'z'))
}
