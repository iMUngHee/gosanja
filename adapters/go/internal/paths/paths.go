// Package paths decides whether a path lies in the analysed root by file identity, so that a
// link or, on a case-insensitive volume, another spelling of the root cannot hide it.
package paths

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// ErrUnresolvable reports a path whose missing part cannot be judged: it names `.` or `..`, it
// continues through something that is not a directory, or its existing part cannot be read.
var ErrUnresolvable = errors.New("cannot be resolved")

// Resolve returns path with its longest existing prefix resolved through links and the missing
// remainder appended. The prefix is found segment by segment on the path as written, so that
// `missing/../x` is judged as written and not as `x`; the remainder may hold no `.` or `..`
// segment, since it cannot be resolved and could climb back. A relative path is taken
// against cwd.
func Resolve(path, cwd string) (string, error) {
	if !filepath.IsAbs(path) {
		path = cwd + string(filepath.Separator) + path
	}
	segments := strings.Split(filepath.ToSlash(path), "/")
	existing := string(filepath.Separator)
	i := 1
	for ; i < len(segments); i++ {
		if segments[i] == "" {
			continue
		}
		next := existing + string(filepath.Separator) + segments[i]
		if existing == string(filepath.Separator) {
			next = existing + segments[i]
		}
		if _, err := os.Lstat(next); err != nil {
			if errors.Is(err, os.ErrNotExist) {
				break
			}
			return "", fmt.Errorf("%s: %w: %v", path, ErrUnresolvable, err)
		}
		existing = next
	}
	var rest []string
	for _, segment := range segments[i:] {
		switch segment {
		case "":
		case ".", "..":
			return "", fmt.Errorf("%s: %w: a `%s` segment in its missing part", path, ErrUnresolvable, segment)
		default:
			rest = append(rest, segment)
		}
	}
	resolved, err := filepath.EvalSymlinks(existing)
	if err != nil {
		return "", fmt.Errorf("%s: %w: %v", path, ErrUnresolvable, err)
	}
	if len(rest) > 0 {
		info, err := os.Stat(resolved)
		if err != nil {
			return "", fmt.Errorf("%s: %w: %v", path, ErrUnresolvable, err)
		}
		if !info.IsDir() {
			return "", fmt.Errorf("%s: %w: %s is not a directory", path, ErrUnresolvable, resolved)
		}
	}
	return filepath.Join(append([]string{resolved}, rest...)...), nil
}

// Within reports whether path is root or lies beneath it. It compares the longest existing
// prefix of path, and each of that prefix's ancestors, with root through os.SameFile, so that
// the missing remainder plays no part. path is a resolved path, as Resolve gives.
func Within(root os.FileInfo, path string) (bool, error) {
	existing := filepath.Clean(path)
	for {
		if _, err := os.Lstat(existing); err == nil {
			break
		}
		parent := filepath.Dir(existing)
		if parent == existing {
			return false, nil
		}
		existing = parent
	}
	for dir := existing; ; dir = filepath.Dir(dir) {
		info, err := os.Stat(dir)
		if err != nil {
			return false, fmt.Errorf("%s: %w: %v", path, ErrUnresolvable, err)
		}
		if os.SameFile(info, root) {
			return true, nil
		}
		if filepath.Dir(dir) == dir {
			return false, nil
		}
	}
}
