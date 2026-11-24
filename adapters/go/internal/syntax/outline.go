package syntax

import (
	"go/ast"
	"go/token"
	"go/types"

	"gosanja/adapters/go/internal/contract"
)

const maxOutlineDepth = 32

type outline struct {
	fset    *token.FileSet
	info    *types.Info
	tooDeep bool
	invalid bool
}

// Walk transparent AST nodes until an outline node takes ownership of its
// descendants. For example, a conversion contributes no node, but calls in its
// arguments remain visible.
func (w *outline) walk(root ast.Node, depth int) []contract.OutlineNode {
	var nodes []contract.OutlineNode
	ast.Inspect(root, func(n ast.Node) bool {
		if n == nil || w.tooDeep || w.invalid {
			return false
		}
		row := w.describe(n)
		if row.K == "" {
			return true
		}
		if depth > maxOutlineDepth {
			w.tooDeep = true
			return false
		}
		row.Range = w.span(n)
		row.Children = w.children(n, depth+1)
		if !rootsInside(row.Range, row.Children) || row.Callee != nil && !contains(row.Range, *row.Callee) {
			w.invalid = true
		}
		nodes = append(nodes, row)
		return false
	})
	return nodes
}

func (w *outline) children(root ast.Node, depth int) []contract.OutlineNode {
	var nodes []contract.OutlineNode
	if branch, ok := root.(*ast.IfStmt); ok && branch.Else != nil {
		if branch.Init != nil {
			nodes = append(nodes, w.walk(branch.Init, depth)...)
		}
		nodes = append(nodes, w.walk(branch.Cond, depth)...)
		nodes = append(nodes, w.walk(branch.Body, depth)...)
		if depth > maxOutlineDepth {
			w.tooDeep = true
			return nodes
		}
		arm := contract.OutlineNode{K: "arm", Detail: "else", Range: w.span(branch.Else)}
		arm.Children = w.walk(branch.Else, depth+1)
		if !rootsInside(arm.Range, arm.Children) {
			w.invalid = true
		}
		return append(nodes, arm)
	}
	ast.Inspect(root, func(n ast.Node) bool {
		if n == root {
			return true
		}
		if n != nil {
			nodes = append(nodes, w.walk(n, depth)...)
		}
		return false
	})
	return nodes
}

func (w *outline) describe(n ast.Node) contract.OutlineNode {
	var node contract.OutlineNode
	switch n := n.(type) {
	case *ast.IfStmt:
		node.K, node.Detail = "branch", "if"
	case *ast.SwitchStmt:
		node.K, node.Detail = "branch", "switch"
	case *ast.TypeSwitchStmt:
		node.K, node.Detail = "branch", "type-switch"
	case *ast.SelectStmt:
		node.K, node.Detail = "branch", "select"
	case *ast.CaseClause, *ast.CommClause:
		node.K = "arm"
	case *ast.ForStmt:
		node.K, node.Detail = "loop", "for"
	case *ast.RangeStmt:
		node.K, node.Detail = "loop", "range"
	case *ast.ReturnStmt:
		node.K, node.Detail = "exit", "return"
	case *ast.BranchStmt:
		node.K, node.Detail = "exit", n.Tok.String()
	case *ast.GoStmt:
		node.K, node.Detail = "spawn", "go"
	case *ast.DeferStmt:
		node.K, node.Detail = "spawn", "defer"
	case *ast.FuncLit:
		node.K = "closure"
	case *ast.CallExpr:
		node.K = "call"
		if w.info == nil {
			return node
		}
		typed := w.info.Types[n.Fun]
		if typed.IsType() {
			return contract.OutlineNode{}
		}
		if _, builtin := w.info.Uses[writtenName(ast.Unparen(n.Fun))].(*types.Builtin); builtin {
			return contract.OutlineNode{}
		}
		callee := calleeName(n.Fun, w.info)
		if callee == nil {
			return node
		}
		if typed.Type != nil {
			_, signature := typed.Type.Underlying().(*types.Signature)
			// The checker can also accept a call through a type parameter
			// with a function constraint, whose underlying type is an interface.
			_, callTyped := w.info.Types[n]
			if signature || callTyped {
				span := w.span(callee)
				node.Callee = &span
			}
		}
	}
	return node
}

func calleeName(expr ast.Expr, info *types.Info) *ast.Ident {
	switch n := expr.(type) {
	case *ast.ParenExpr:
		name, _ := ast.Unparen(n).(*ast.Ident)
		return name
	case *ast.IndexExpr:
		if name := writtenName(n.X); name != nil {
			if _, instantiated := info.Instances[name]; instantiated {
				return name
			}
		}
	case *ast.IndexListExpr:
		if name := writtenName(n.X); name != nil {
			if _, instantiated := info.Instances[name]; instantiated {
				return name
			}
		}
	default:
		return writtenName(expr)
	}
	return nil
}

func writtenName(expr ast.Expr) *ast.Ident {
	switch n := expr.(type) {
	case *ast.Ident:
		return n
	case *ast.SelectorExpr:
		return n.Sel
	}
	return nil
}

func (w *outline) span(n ast.Node) contract.Range {
	start := w.fset.PositionFor(n.Pos(), false)
	end := w.fset.PositionFor(n.End(), false)
	sl, sc, startOK := zeroBased(start)
	el, ec, endOK := zeroBased(end)
	r := contract.Range{sl, sc, el, ec}
	if !startOK || !endOK || start.Filename != end.Filename || !ordered(sl, sc, el, ec) {
		w.invalid = true
	}
	return r
}

func rootsInside(extent contract.Range, nodes []contract.OutlineNode) bool {
	for _, node := range nodes {
		if !contains(extent, node.Range) {
			return false
		}
	}
	return true
}

func contains(outer, inner contract.Range) bool {
	return ordered(outer[0], outer[1], inner[0], inner[1]) &&
		ordered(inner[2], inner[3], outer[2], outer[3])
}

func ordered(al, ac, bl, bc uint32) bool {
	return al < bl || al == bl && ac <= bc
}
