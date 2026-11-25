// Package contract holds the adapter contract's types (spec/adapter-contract.md, version 0.4)
// as this adapter writes them. Field order follows the specification, and the tests check every
// written file against spec/schema.
package contract

import (
	"fmt"
	"strings"
)

// Version is the adapter contract version the adapter speaks.
const Version = "0.4"

// Layer names of the contract.
const (
	Base   = "base"
	Syntax = "syntax"
	Calls  = "calls"
)

// Manifest is what `describe` prints.
type Manifest struct {
	Contract         string        `json:"contract"`
	Name             string        `json:"name"`
	Version          string        `json:"version"`
	Languages        []string      `json:"languages"`
	PositionEncoding string        `json:"position_encoding"`
	Layers           []string      `json:"layers"`
	Requires         []Requirement `json:"requires,omitempty"`
	Markers          []string      `json:"markers,omitempty"`
}

// Requirement is an external tool the adapter needs at run time.
type Requirement struct {
	Tool    string `json:"tool"`
	Version string `json:"version,omitempty"`
}

// Result is result.json. encoding/json writes the keys of Layers in sorted order.
type Result struct {
	Contract string           `json:"contract"`
	Layers   map[string]Entry `json:"layers"`
	Tools    []Tool           `json:"tools,omitempty"`
}

// Entry is the result of one layer.
type Entry struct {
	Status      string       `json:"status"`
	Diagnostics []Diagnostic `json:"diagnostics,omitempty"`
	Skipped     []Skipped    `json:"skipped,omitempty"`
	Merged      []string     `json:"merged,omitempty"` // global SCIP symbols, in a successful base entry only
}

// Diagnostic names a kind of problem. The adapter writes the code alone, so that a failed
// result is as repeatable as a successful one.
type Diagnostic struct {
	Code string `json:"code"`
}

// Skipped is a file a layer was meant to cover and could not process, in whole or in part.
type Skipped struct {
	File string `json:"file"`
	Code string `json:"code"`
}

// Tool is a tool that was run, with the version it reported.
type Tool struct {
	Name    string `json:"name"`
	Version string `json:"version"`
}

// OK is a successful entry with the files it skipped.
func OK(skipped []Skipped) Entry { return Entry{Status: "ok", Skipped: skipped} }

// Failed is a failed entry with one diagnostic code.
func Failed(code string) Entry {
	return Entry{Status: "failed", Diagnostics: []Diagnostic{{Code: code}}}
}

// Request is the set of layers a run is asked for; it always holds the base layer.
type Request struct {
	Syntax bool
	Calls  bool
}

// ParseRequest reads a `--layers` list: layer names separated by commas, each at most once,
// with no space and no empty element, `base` among them, and only layers in offered.
func ParseRequest(list string, offered []string) (Request, error) {
	var request Request
	seen := map[string]bool{}
	for _, name := range strings.Split(list, ",") {
		known := false
		for _, o := range offered {
			known = known || o == name
		}
		if !known {
			return Request{}, fmt.Errorf("unknown layer %q in --layers %q", name, list)
		}
		if seen[name] {
			return Request{}, fmt.Errorf("duplicate layer %q in --layers %q", name, list)
		}
		seen[name] = true
		switch name {
		case Syntax:
			request.Syntax = true
		case Calls:
			request.Calls = true
		}
	}
	if !seen[Base] {
		return Request{}, fmt.Errorf("--layers %q must include %q", list, Base)
	}
	return request, nil
}

// SyntaxFact is one row of syntax.jsonl.
type SyntaxFact struct {
	Anchor     Anchor        `json:"anchor"`
	Attributes []string      `json:"attributes,omitempty"`
	Outline    []OutlineNode `json:"outline,omitempty"`
}

// Anchor points at a definition by the position of its name token, zero-based, the column in
// UTF-8 bytes. The column is always written, 0 included.
type Anchor struct {
	File string `json:"file"`
	Line uint32 `json:"line"`
	Col  uint32 `json:"col"`
}

// Range is [startLine, startCol, endLine, endCol], zero-based, end exclusive.
type Range [4]uint32

// OutlineNode is one node of a control-flow outline.
type OutlineNode struct {
	K        string        `json:"k"`
	Detail   string        `json:"detail,omitempty"`
	Range    Range         `json:"range"`
	Callee   *Range        `json:"callee,omitempty"`
	Children []OutlineNode `json:"children,omitempty"`
}
