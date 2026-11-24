package syntax

import (
	"go/token"
	"math"
	"path/filepath"
	"strconv"
	"strings"

	"golang.org/x/tools/go/packages"
)

const errorLimit = "too many errors"

type diagnostic struct {
	pos  token.Position
	text string
}

// Compare compiler listings in remapped coordinates and mark skips in physical coordinates.
func (s *selection) account(pkg *packages.Package, files, parsed map[string]bool) {
	var typed []diagnostic
	for _, err := range pkg.TypeErrors {
		if err.Fset == nil {
			continue
		}
		pos := err.Fset.PositionFor(err.Pos, true)
		pos.Filename = absolute(s.in.Root, pos.Filename)
		typed = append(typed, diagnostic{pos, err.Msg})
		file := relativeSource(s.in.Root, err.Fset.PositionFor(err.Pos, false).Filename)
		if files[file] {
			s.skip(file, "type-error")
		}
	}

	var compiler []diagnostic
	var other []packages.Error
	for _, err := range pkg.Errors {
		if err.Kind == packages.ListError && strings.HasPrefix(err.Msg, "# ") {
			compiler = append(compiler, compilerDiagnostics(s.in.Root, err.Msg)...)
		} else {
			other = append(other, err)
		}
	}
	physical := true
	for file := range files {
		physical = physical && !s.in.Files[file].HasLineDirective
	}
	// The compiler and checker each report these errors once.
	listing := make(matcher, len(typed))
	structured := make(matcher, len(typed))
	allAccounted := true
	for _, item := range compiler {
		if item.text == errorLimit && len(pkg.TypeErrors) > 0 ||
			listing.take(typed, item, true) || physical && s.namesFailedRead(files, item) {
			continue
		}
		allAccounted = false
	}
	for _, err := range other {
		item := diagnostic{parsePosition(s.in.Root, err.Pos), err.Msg}
		if err.Kind == packages.TypeError && structured.take(typed, item, false) {
			continue
		}
		if physical {
			if s.namesFailedRead(files, item) {
				continue
			}
			file := relativeSource(s.in.Root, item.pos.Filename)
			if files[file] && parsed[file] && pkg.TypesInfo != nil && s.in.Files[file].Code == "" {
				s.skip(file, "package-error")
				continue
			}
		}
		allAccounted = false
	}
	if !allAccounted {
		for file := range files {
			s.skip(file, "package-error")
			s.suppressed[file] = true
		}
	}
}

// An unpositioned file-system diagnostic can still name the failed source in its text.
func (s *selection) namesFailedRead(files map[string]bool, item diagnostic) bool {
	name := relativeSource(s.in.Root, item.pos.Filename)
	if files[name] && s.in.Files[name].Code != "" {
		return true
	}
	operation, tail, _ := strings.Cut(item.text, " ")
	switch operation {
	case "open", "read", "stat", "lstat":
		filename, _, ok := strings.Cut(tail, ": ")
		name = relativeSource(s.in.Root, filename)
		return ok && files[name] && s.in.Files[name].Code != ""
	}
	return false
}

type matcher []bool

func (m matcher) take(typed []diagnostic, actual diagnostic, compiler bool) bool {
	best, distance := -1, math.MaxInt
	for i, entry := range typed {
		if m[i] || entry.text != actual.text || entry.pos.Filename != actual.pos.Filename ||
			entry.pos.Line != actual.pos.Line {
			continue
		}
		c := actual.pos.Column
		if !compiler && c != 0 && c != entry.pos.Column {
			continue
		}
		// A missing column in the listing imposes no preference. A candidate
		// with no column is usable, but ranks behind positioned candidates.
		delta := 0
		if c != 0 {
			delta = max(entry.pos.Column-c, c-entry.pos.Column)
			if entry.pos.Column == 0 {
				delta = math.MaxInt
			}
		}
		if best < 0 || delta < distance {
			best, distance = i, delta
		}
		if distance == 0 {
			break
		}
	}
	if best < 0 {
		return false
	}
	m[best] = true
	return true
}

func compilerDiagnostics(root, block string) []diagnostic {
	_, body, _ := strings.Cut(strings.TrimSuffix(block, "\n"), "\n")
	if body == "" {
		return []diagnostic{{text: block}}
	}
	var result []diagnostic
	for line := range strings.SplitSeq(body, "\n") {
		content := strings.TrimLeft(line, "\t")
		prefix := line[:len(line)-len(content)]
		loc, message, hasMessage := strings.Cut(content, ": ")
		pos := parsePosition(root, loc)
		if content == errorLimit || hasMessage && pos.IsValid() && message == errorLimit {
			return append(result, diagnostic{text: errorLimit})
		}
		if hasMessage && pos.IsValid() {
			result = append(result, diagnostic{pos, prefix + message})
		} else if prefix != "" && len(result) != 0 {
			result[len(result)-1].text += "\n" + line
		} else {
			result = append(result, diagnostic{text: line})
		}
	}
	return result
}

func parsePosition(root, text string) token.Position {
	if strings.HasSuffix(text, "]") {
		if bracket := strings.LastIndexByte(text, '['); bracket >= 0 {
			text = text[:bracket]
		}
	}
	file, last, ok := trailingNumber(text)
	if !ok || file == "" {
		return token.Position{}
	}
	pos := token.Position{Filename: file, Line: last}
	if name, line, ok := trailingNumber(file); ok && name != "" {
		pos.Filename, pos.Line, pos.Column = name, line, last
	}
	pos.Filename = absolute(root, pos.Filename)
	return pos
}

func trailingNumber(text string) (string, int, bool) {
	i := strings.LastIndexByte(text, ':')
	if i < 0 {
		return "", 0, false
	}
	n, err := strconv.Atoi(text[i+1:])
	return text[:i], n, err == nil && n > 0
}

func absolute(root, filename string) string {
	if filename == "" {
		return ""
	}
	if filepath.IsAbs(filename) {
		return filepath.Clean(filename)
	}
	return filepath.Join(root, filename)
}
