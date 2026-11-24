package syntax

import (
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"math"
	"strings"

	"gosanja/adapters/go/internal/contract"
)

func (s *selection) items(file string, syntax *ast.File, fset *token.FileSet, info *types.Info) []contract.SyntaxFact {
	var rows []contract.SyntaxFact
	seen := map[contract.Anchor]bool{}
	add := func(name *ast.Ident, attrs []string, body *ast.BlockStmt) {
		if name.Name == "_" {
			return
		}
		pos := fset.PositionFor(name.Pos(), false)
		line, col, ok := zeroBased(pos)
		if !ok {
			return
		}
		anchor := contract.Anchor{File: file, Line: line, Col: col}
		if seen[anchor] {
			s.skip(file, "duplicate-anchor")
			if s.in.Stderr != nil {
				fmt.Fprintf(s.in.Stderr, "duplicate syntax anchor for %s at %s:%d:%d\n", name.Name, file, pos.Line, pos.Column)
			}
			return
		}
		seen[anchor] = true
		row := contract.SyntaxFact{Anchor: anchor, Attributes: attrs}
		if body != nil && s.extents != nil {
			walker := outline{fset: fset, info: info}
			nodes := walker.walk(body, 1)
			extent, found := s.extents[anchor]
			if walker.tooDeep {
				s.skip(file, "outline-too-deep")
			} else if walker.invalid || len(nodes) > 0 && (!found || !rootsInside(extent, nodes)) {
				s.skip(file, "outline-outside-definition")
			} else {
				row.Outline = nodes
			}
		}
		rows = append(rows, row)
	}

	if tag := buildTag(syntax); tag != "" {
		add(syntax.Name, []string{tag}, nil)
	}
	for _, declaration := range syntax.Decls {
		switch decl := declaration.(type) {
		case *ast.FuncDecl:
			add(decl.Name, directives(decl.Doc), decl.Body)
		case *ast.GenDecl:
			for _, spec := range decl.Specs {
				var doc *ast.CommentGroup
				if !decl.Lparen.IsValid() {
					doc = decl.Doc
				}
				switch item := spec.(type) {
				case *ast.TypeSpec:
					if item.Doc != nil {
						doc = item.Doc
					}
					add(item.Name, directives(doc), nil)
					if iface, ok := item.Type.(*ast.InterfaceType); ok {
						for _, field := range iface.Methods.List {
							for _, name := range field.Names {
								add(name, directives(field.Doc), nil)
							}
						}
					}
				case *ast.ValueSpec:
					if item.Doc != nil {
						doc = item.Doc
					}
					attrs := directives(doc)
					for _, name := range item.Names {
						add(name, attrs, nil)
					}
				}
			}
		}
	}
	return rows
}

func zeroBased(pos token.Position) (line, col uint32, ok bool) {
	if !pos.IsValid() || pos.Column < 1 || uint64(pos.Line-1) > math.MaxUint32 || uint64(pos.Column-1) > math.MaxUint32 {
		return 0, 0, false
	}
	return uint32(pos.Line - 1), uint32(pos.Column - 1), true
}

func buildTag(file *ast.File) string {
	for _, group := range file.Comments {
		if group.Pos() >= file.Package {
			break
		}
		for _, comment := range group.List {
			if strings.HasPrefix(comment.Text, "//go:build ") || strings.HasPrefix(comment.Text, "//go:build\t") {
				return comment.Text
			}
		}
	}
	return ""
}

func directives(group *ast.CommentGroup) []string {
	var attrs []string
	if group != nil {
		for _, comment := range group.List {
			if strings.HasPrefix(comment.Text, "//go:") {
				attrs = append(attrs, comment.Text)
			}
		}
	}
	return attrs
}
