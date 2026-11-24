package base

import (
	"bytes"
	"go/build"
	"go/parser"
	"go/token"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"unicode/utf8"

	"gosanja/adapters/go/internal/paths"
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
}

// readFile reads one module file through the root boundary and parses it with go/parser
// alone. A link that leaves the root, or anything but a regular file, is unreadable.
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
	if _, err := parser.ParseFile(token.NewFileSet(), f.abs, src, parser.SkipObjectResolution); err != nil {
		read.Code = ParseFailed
	}
	return read
}

// buildContext is the context of owner decision 5: the host's GOOS and GOARCH, cgo off, and
// the release tags of the go command that lists the packages, whose version word is goVersion
// (`go1.27.3`, possibly with a suffix). The adapter's own build tags would judge a
// `//go:build go1.27` file by the compiler that built the adapter.
func buildContext(goVersion string) build.Context {
	ctxt := build.Default
	ctxt.GOOS, ctxt.GOARCH = runtime.GOOS, runtime.GOARCH
	ctxt.CgoEnabled = false
	if tags, ok := releaseTags(goVersion); ok {
		ctxt.ReleaseTags = tags
	}
	return ctxt
}

// releaseTags are go1.1 up to the minor release goVersion names, as the go command sets them.
func releaseTags(goVersion string) ([]string, bool) {
	rest, ok := strings.CutPrefix(goVersion, "go1.")
	if !ok {
		return nil, false
	}
	end := strings.IndexFunc(rest, func(r rune) bool { return r < '0' || r > '9' })
	if end == 0 {
		return nil, false
	}
	if end > 0 {
		rest = rest[:end]
	}
	minor, err := strconv.Atoi(rest)
	if err != nil || minor < 1 {
		return nil, false
	}
	tags := make([]string, 0, minor)
	for i := 1; i <= minor; i++ {
		tags = append(tags, "go1."+strconv.Itoa(i))
	}
	return tags, true
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
func unnamed(root string, inv *Inventory, ignored []string, goVersion string) (constrained, missed []string, err error) {
	named := inv.named()
	ctxt := buildContext(goVersion)
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
