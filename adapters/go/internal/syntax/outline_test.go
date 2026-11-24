package syntax

import (
	"fmt"
	"path/filepath"
	"strings"
	"testing"

	"github.com/scip-code/scip/bindings/go/scip"
	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/scipindex"
)

func TestOutline(t *testing.T) {
	in := fixtureInput(t)
	index, err := scipindex.Read(filepath.Join(in.Root, "index.scip"))
	if err != nil {
		t.Fatal(err)
	}
	in.Index = index
	status, rows := Layer(t.Context(), in)
	wantLayer(t, status, rows, nil, []string{"external_test.go", "flow.go", "flow_test.go", "meter/meter.go", "tagged.go", "zz_generated.go"})
	flow := outlineAt(t, rows, "flow.go", 60, 5)
	const want = `call [61 14 61 36] callee=[61 20 61 26]
closure [62 20 62 49]
  exit:return [62 33 62 47]
    call [62 40 62 47] callee=[62 40 62 44]
call [63 32 63 46] callee=[63 32 63 35]
call [63 48 63 68] callee=[63 54 63 57]
call [63 70 63 85] callee=[63 76 63 79]
call [63 87 63 97]
  call [63 87 63 94] callee=[63 87 63 92]
call [63 99 63 106]
call [65 19 65 34] callee=[65 19 65 23]
spawn:go [67 1 67 16]
  call [67 4 67 16] callee=[67 4 67 10]
spawn:defer [68 1 68 24]
  call [68 7 68 24] callee=[68 7 68 18]
branch:if [70 1 77 2]
  exit:goto [72 2 72 15]
  arm:else [73 8 77 2]
    branch:if [73 8 77 2]
      arm:else [75 8 77 2]
branch:switch [80 1 88 2]
  arm [81 1 83 13]
    call [82 11 82 18] callee=[82 11 82 15]
    exit:fallthrough [83 2 83 13]
  arm [84 1 85 18]
    call [85 11 85 18] callee=[85 11 85 15]
  arm [86 1 87 18]
    call [87 11 87 18] callee=[87 11 87 15]
branch:type-switch [90 1 97 2]
  arm [91 1 92 16]
  arm [93 1 94 21]
  arm [95 1 96 19]
    call [96 11 96 19] callee=[96 11 96 15]
loop:for [100 1 108 2]
  branch:if [101 2 103 3]
    exit:continue [102 3 102 11]
  branch:if [104 2 106 3]
    exit:break [105 3 105 8]
loop:range [109 1 111 2]
branch:select [113 1 118 2]
  arm [114 1 115 16]
  arm [116 1 117 19]
    call [117 11 117 19] callee=[117 11 117 15]
closure [120 9 120 51]
  exit:return [120 31 120 49]
    call [120 38 120 49] callee=[120 38 120 42]
exit:return [121 1 121 19]
  call [121 8 121 19] callee=[121 8 121 12]
`
	if got := renderOutline(flow); got != want {
		t.Fatalf("Flow outline:\n%s", got)
	}
	generated := outlineAt(t, rows, "zz_generated.go", 5, 5)
	if got := renderOutline(generated); got != "exit:return [5 23 5 38]\n  call [5 30 5 38] callee=[5 30 5 34]\n" {
		t.Fatalf("Generated outline:\n%s", got)
	}
}

func TestOutlineFailuresKeepDeclaration(t *testing.T) {
	for _, test := range []struct {
		name string
		n    int
		code string
	}{
		{"depth-32", 32, ""},
		{"depth-33", 33, "outline-too-deep"},
		{"outside-definition", 1, "outline-outside-definition"},
		{"missing-definition", 1, "outline-outside-definition"},
	} {
		t.Run(test.name, func(t *testing.T) {
			const filename = "p.go"
			src := "package p\nfunc step(value int) int { return value }\nfunc F() { " +
				strings.Repeat("step(", test.n) + "1" + strings.Repeat(")", test.n) + " }\n"
			in := tempModule(t, map[string]string{filename: src}, "")
			extent := []int32{2, 0, 2, int32(len(strings.Split(src, "\n")[2]))}
			if test.code == "outline-outside-definition" {
				extent[3] = 10
			}
			definitions := []*scip.Occurrence{testDefinition("step", []int32{1, 5, 9}, []int32{1, 0, 1, 41})}
			if test.name != "missing-definition" {
				definitions = append(definitions, testDefinition("F", []int32{2, 5, 6}, extent))
			}
			in.Index = outlineIndex(filename, definitions...)
			status, rows := Layer(t.Context(), in)
			if len(rows) != 2 {
				t.Fatalf("wanted both declaration rows, got %+v", rows)
			}
			var skipped []contract.Skipped
			if test.code != "" {
				skipped = []contract.Skipped{{File: filename, Code: test.code}}
			}
			wantLayer(t, status, rows, skipped, []string{filename})
			nodes := outlineAt(t, rows, filename, 2, 5)
			if test.code != "" {
				if len(nodes) != 0 {
					t.Fatalf("bad outline retained: %s", renderOutline(nodes))
				}
				return
			}
			for level := 1; level <= test.n; level++ {
				if len(nodes) != 1 || nodes[0].K != "call" || nodes[0].Callee == nil {
					t.Fatalf("depth %d: expected one named call, got %+v", level, nodes)
				}
				nodes = nodes[0].Children
			}
			if len(nodes) != 0 {
				t.Fatalf("extra call descendants: %+v", nodes)
			}
		})
	}
}

func TestOutlineAdditionalFunctionForms(t *testing.T) {
	const filename = "p.go"
	const src = "package p\n" +
		"func Apply[Fn ~func()](f Fn) { (f)() }\n" +
		"func pick[A, B any](a A, b B) A { return a }\n" +
		"func Use() { pick[int, string](1, \"one\") }\n"
	in := tempModule(t, map[string]string{filename: src}, "")
	lines := strings.Split(src, "\n")
	in.Index = outlineIndex(filename,
		testDefinition("Apply", []int32{1, 5, 10}, []int32{1, 0, 1, int32(len(lines[1]))}),
		testDefinition("pick", []int32{2, 5, 9}, []int32{2, 0, 2, int32(len(lines[2]))}),
		testDefinition("Use", []int32{3, 5, 8}, []int32{3, 0, 3, int32(len(lines[3]))}),
	)
	status, rows := Layer(t.Context(), in)
	wantLayer(t, status, rows, nil, []string{filename})
	apply := renderOutline(outlineAt(t, rows, filename, 1, 5))
	use := renderOutline(outlineAt(t, rows, filename, 3, 5))
	if apply != "call [1 31 1 36] callee=[1 32 1 33]\n" || use != "call [3 13 3 40] callee=[3 13 3 17]\n" {
		t.Fatalf("Apply outline %q, Use outline %q", apply, use)
	}
}

func TestOutlineUntypedCallKeepsArguments(t *testing.T) {
	const filename = "p.go"
	in := tempModule(t, map[string]string{filename: "package p\nfunc Good() int { return 1 }\nfunc F() { Unknown(Good()) }\n"}, "")
	in.Index = outlineIndex(filename,
		testDefinition("Good", []int32{1, 5, 9}, []int32{1, 0, 1, 28}),
		testDefinition("F", []int32{2, 5, 6}, []int32{2, 0, 2, 28}),
	)
	status, rows := Layer(t.Context(), in)
	wantLayer(t, status, rows, []contract.Skipped{{File: filename, Code: "type-error"}}, []string{filename})
	want := "call [2 11 2 26]\n  call [2 19 2 25] callee=[2 19 2 23]\n"
	if got := renderOutline(outlineAt(t, rows, filename, 2, 5)); got != want {
		t.Fatalf("untyped call:\n%s", got)
	}
}

func TestDefinitionExtents(t *testing.T) {
	index := outlineIndex("p.go",
		testDefinition("F", []int32{2, 5, 6}, []int32{2, 0, 2, 12}),
		testDefinition("F", []int32{2, 5, 6}, []int32{2, 0, 2, 15}),
		testDefinition("G", []int32{4, 5, 6}, []int32{4, 0, 4, 12}),
		testDefinition("H", []int32{4, 5, 7}, []int32{4, 0, 4, 13}),
		testDefinition("NoRange", []int32{6, 5, 6}, []int32{6, 0, 6, -1}),
	)
	for _, list := range [][]*scip.Occurrence{
		index.Documents[0].Occurrences,
		{
			index.Documents[0].Occurrences[1], index.Documents[0].Occurrences[0],
			index.Documents[0].Occurrences[3], index.Documents[0].Occurrences[2],
			index.Documents[0].Occurrences[4],
		},
	} {
		index.Documents[0].Occurrences = list
		extents := definitionExtents(index)
		if len(extents) != 1 || extents[contract.Anchor{File: "p.go", Line: 2, Col: 5}] != (contract.Range{2, 0, 2, 15}) {
			t.Fatalf("extents after exact duplicate, ambiguous and negative spans: %+v", extents)
		}
	}
}

func outlineAt(t *testing.T, rows []contract.SyntaxFact, file string, line, col uint32) []contract.OutlineNode {
	t.Helper()
	for _, row := range rows {
		if row.Anchor == (contract.Anchor{File: file, Line: line, Col: col}) {
			return row.Outline
		}
	}
	t.Fatalf("missing definition at %s:%d:%d", file, line, col)
	return nil
}

func outlineIndex(file string, definitions ...*scip.Occurrence) *scipindex.Index {
	return &scipindex.Index{Documents: []*scip.Document{{
		RelativePath: file, PositionEncoding: scip.PositionEncoding_UTF8CodeUnitOffsetFromLineStart, Occurrences: definitions,
	}}}
}

func testDefinition(name string, token, extent []int32) *scip.Occurrence {
	return &scip.Occurrence{
		Symbol:      "scip-go gomod example.com/diagnostics v0.0.0 p/" + name + "().",
		SymbolRoles: int32(scip.SymbolRole_Definition),
		TypedRange: &scip.Occurrence_SingleLineRange{
			SingleLineRange: &scip.SingleLineRange{Line: token[0], StartCharacter: token[1], EndCharacter: token[2]},
		},
		TypedEnclosingRange: &scip.Occurrence_SingleLineEnclosingRange{
			SingleLineEnclosingRange: &scip.SingleLineRange{Line: extent[0], StartCharacter: extent[1], EndCharacter: extent[3]},
		},
	}
}

func renderOutline(nodes []contract.OutlineNode) string {
	var out strings.Builder
	var visit func([]contract.OutlineNode, string)
	visit = func(nodes []contract.OutlineNode, indent string) {
		for _, node := range nodes {
			fmt.Fprintf(&out, "%s%s", indent, node.K)
			if node.Detail != "" {
				fmt.Fprintf(&out, ":%s", node.Detail)
			}
			fmt.Fprintf(&out, " %v", node.Range)
			if node.Callee != nil {
				fmt.Fprintf(&out, " callee=%v", *node.Callee)
			}
			out.WriteByte('\n')
			visit(node.Children, indent+"  ")
		}
	}
	visit(nodes, "")
	return out.String()
}
