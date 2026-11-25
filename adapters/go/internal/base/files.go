package base

import (
	"bytes"
	"context"
	"go/build"
	"go/parser"
	"go/scanner"
	"go/token"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"unicode/utf8"

	"gosanja/adapters/go/internal/paths"
	"gosanja/adapters/go/internal/scipindex"
)

// Codes of the adapter's own read of a module file; "" is readable.
const (
	Unreadable  = "unreadable"
	InvalidUTF8 = "invalid-utf8"
	ParseFailed = "parse-failed"
)

// Read is the outcome of the adapter's own read of one module file.
type Read struct {
	Code string
	// LineDirective reports a `//line ` or `/*line ` in the raw bytes, found before parsing.
	LineDirective bool
	// PackageName and PackageRange record a valid clause independently of Code.
	PackageName  string
	PackageRange scipindex.Range
}

// readFile reads and parses one module file through the root boundary.
// A link that leaves the root, or anything but a regular file, is unreadable.
func readFile(root os.FileInfo, f file) Read {
	resolved, err := filepath.EvalSymlinks(f.abs)
	if err != nil {
		return Read{Code: Unreadable}
	}
	inside, err := paths.Within(root, resolved)
	if err != nil || !inside {
		return Read{Code: Unreadable}
	}
	if info, err := os.Stat(resolved); err != nil || !info.Mode().IsRegular() {
		return Read{Code: Unreadable}
	}
	src, err := os.ReadFile(resolved)
	if err != nil {
		return Read{Code: Unreadable}
	}
	read := Read{LineDirective: bytes.Contains(src, []byte("//line ")) || bytes.Contains(src, []byte("/*line "))}
	if !utf8.Valid(src) {
		read.Code = InvalidUTF8
		return read
	}
	fset := token.NewFileSet()
	tree, err := parser.ParseFile(fset, f.abs, src, parser.SkipObjectResolution)
	var name string
	var pos token.Pos
	if err == nil {
		name, pos = tree.Name.Name, tree.Name.Pos()
	} else {
		read.Code = ParseFailed
		// The indexer can recover the package clause. Stop at its semicolon so a
		// scanner error in the next declaration cannot invalidate the clause.
		var lex scanner.Scanner
		lex.Init(fset.AddFile(f.abs, -1, len(src)), src, nil, 0)
		_, keyword, _ := lex.Scan()
		identifierPos, identifier, text := lex.Scan()
		_, terminator, _ := lex.Scan()
		if keyword != token.PACKAGE || identifier != token.IDENT || terminator != token.SEMICOLON || lex.ErrorCount != 0 {
			return read
		}
		name, pos = text, identifierPos
	}
	if name == "_" {
		return read
	}
	start := fset.PositionFor(pos, false)
	line, col := int32(start.Line-1), int32(start.Column-1)
	read.PackageName = name
	read.PackageRange = scipindex.Range{line, col, line, col + int32(len(name))}
	return read
}

// buildContext uses the tool and release tags of the same Go command and environment as
// the inventory. The parent adapter process can have different architecture or experiment tags.
func buildContext(ctx context.Context, root, goTool string, vars []string) (build.Context, error) {
	const format = `{{with context}}{{range .ToolTags}}{{printf "%s " .}}{{end}}{{"\n"}}{{range .ReleaseTags}}{{printf "%s " .}}{{end}}{{end}}`
	stdout, stderr, err := runIn(ctx, root, vars, goTool, "list", "-f", format, "unsafe")
	if err != nil {
		return build.Context{}, fail("dependencies-unreadable", "go list build context: %v: %s", err, strings.TrimSpace(string(stderr)))
	}
	tags := strings.Split(strings.TrimSuffix(string(stdout), "\n"), "\n")
	if len(tags) != 2 || len(strings.Fields(tags[1])) == 0 {
		return build.Context{}, fail("dependencies-unreadable", "go list returned an invalid build context")
	}
	ctxt := build.Default
	ctxt.GOOS, ctxt.GOARCH = runtime.GOOS, runtime.GOARCH
	ctxt.CgoEnabled = false
	ctxt.ToolTags = strings.Fields(tags[0])
	ctxt.ReleaseTags = strings.Fields(tags[1])
	return ctxt, nil
}

// ignoredDir reports whether the go command skips dir, relative to the root and written with
// slashes, for one of go.mod's ignore patterns: `./x` names x at the root and what lies
// beneath it, `x` any directory named x at any depth (cmd/go/internal/search).
func ignoredDir(dir string, patterns []string) bool {
	normal := func(p string) string { return "/" + strings.Trim(p, "/") + "/" }
	d := normal(dir)
	for _, pattern := range patterns {
		pattern = filepath.ToSlash(pattern)
		if rel, ok := strings.CutPrefix(pattern, "./"); ok {
			if strings.HasPrefix(d, normal(rel)) {
				return true
			}
		} else if strings.Contains(d, normal(pattern)) {
			return true
		}
	}
	return false
}

// unnamed walks the root for `.go` files that no package of the inventory names, in the
// directories the go tool considers, and sorts them: excluded by the build context, or built
// and so missed by the inventory.
func unnamed(ctx context.Context, root string, inv *Inventory, ignored []string, goTool string, vars []string) (constrained, missed []string, err error) {
	named := inv.named()
	loadContext := sync.OnceValues(func() (build.Context, error) { return buildContext(ctx, root, goTool, vars) })
	imported := map[string]*build.Package{}
	err = filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			name := d.Name()
			rel, _ := filepath.Rel(root, path)
			if path != root && (name == "testdata" || name == "vendor" || strings.HasPrefix(name, ".") ||
				strings.HasPrefix(name, "_") || ignoredDir(filepath.ToSlash(rel), ignored)) {
				return filepath.SkipDir
			}
			if path != root {
				if _, err := os.Stat(filepath.Join(path, "go.mod")); err == nil {
					return filepath.SkipDir
				}
			}
			return nil
		}
		if base := d.Name(); !strings.HasSuffix(base, ".go") || strings.HasPrefix(base, ".") ||
			strings.HasPrefix(base, "_") || named[path] {
			return nil
		}
		dir := filepath.Dir(path)
		pkg, ok := imported[dir]
		if !ok {
			ctxt, err := loadContext()
			if err != nil {
				return err
			}
			pkg, _ = ctxt.ImportDir(dir, 0)
			imported[dir] = pkg
		}
		rel, _ := filepath.Rel(root, path)
		rel = filepath.ToSlash(rel)
		if pkg != nil && contains(pkg.IgnoredGoFiles, filepath.Base(path)) {
			constrained = append(constrained, rel)
		} else {
			missed = append(missed, rel)
		}
		return nil
	})
	return constrained, missed, err
}

func contains(list []string, s string) bool {
	for _, x := range list {
		if x == s {
			return true
		}
	}
	return false
}
