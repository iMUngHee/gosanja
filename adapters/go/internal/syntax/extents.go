package syntax

import (
	"slices"
	"strings"

	"github.com/scip-code/scip/bindings/go/scip"
	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/scipindex"
)

func definitionExtents(index *scipindex.Index) map[contract.Anchor]contract.Range {
	if index == nil {
		return nil
	}
	type site struct {
		symbol string
		name   contract.Range
	}
	sites := map[contract.Anchor]site{}
	ambiguous := map[contract.Anchor]bool{}
	result := map[contract.Anchor]contract.Range{}
	for _, document := range index.Documents {
		if document.PositionEncoding != scip.PositionEncoding_UTF8CodeUnitOffsetFromLineStart {
			continue
		}
		for _, occurrence := range document.Occurrences {
			if !scip.SymbolRole_Definition.Matches(occurrence) || occurrence.Symbol == "" ||
				strings.HasPrefix(occurrence.Symbol, "local ") {
				continue
			}
			rawName, hasName := scipindex.OccurrenceRange(occurrence)
			if !hasName {
				continue
			}
			name, valid := nonnegativeRange(rawName)
			if !valid {
				continue
			}
			a := contract.Anchor{File: document.RelativePath, Line: name[0], Col: name[1]}
			current := site{occurrence.Symbol, name}
			if previous, exists := sites[a]; exists && current != previous {
				ambiguous[a] = true
			}
			sites[a] = current
			if rawBody, hasBody := scipindex.EnclosingRange(occurrence); hasBody {
				if body, valid := nonnegativeRange(rawBody); valid {
					if previous, exists := result[a]; !exists || slices.Compare(previous[:], body[:]) < 0 {
						result[a] = body
					}
				}
			}
		}
	}
	for a := range ambiguous {
		delete(result, a)
	}
	return result
}

func nonnegativeRange(raw scipindex.Range) (contract.Range, bool) {
	var r contract.Range
	for i, n := range raw {
		if n < 0 {
			return contract.Range{}, false
		}
		r[i] = uint32(n)
	}
	return r, ordered(r[0], r[1], r[2], r[3])
}
